//! TEAM-P13 live replay driver. Transport only; the Agent clients decide and act.
use awr_team_pg::{
    DependencyEdge, ExecutionStore, GraphStore, ImportStore, IngestRequest, LeaseStore, ReadStore,
    ReviewStore, SourceFile, SourceStore, TeamStore,
};
use serde_json::{Value, json};

const TENANT: &str = "tenant-t13";

/// Connection handling is Config-native: the raw URL is parsed once, the
/// database comes from TC_DB (falling back to the URL's dbname), and the
/// app role is applied as a config override. Nothing is re-serialized, so
/// IPv6, hostaddr and Unix socket targets keep their meaning (CR #56 r3).
fn base_config() -> tokio_postgres::config::Config {
    let raw = std::env::var("AWR_TEAM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:awr-test@127.0.0.1:55432/awr_team_test".into());
    raw.parse().expect("invalid AWR_TEAM_DATABASE_URL")
}
fn db_name(config: &tokio_postgres::config::Config) -> String {
    std::env::var("TC_DB")
        .unwrap_or_else(|_| config.get_dbname().unwrap_or("awr_team_test").to_string())
}
fn admin_config() -> tokio_postgres::config::Config {
    let mut c = base_config();
    let db = db_name(&c);
    c.dbname(&db);
    c
}
fn app_config() -> tokio_postgres::config::Config {
    let mut c = admin_config();
    c.user("awr_app");
    c.password("app-test");
    c
}
fn leases() -> LeaseStore {
    LeaseStore::from_config(app_config())
}
fn reviews() -> ReviewStore {
    ReviewStore::from_config(app_config())
}
fn executions() -> ExecutionStore {
    ExecutionStore::from_config(app_config())
}
fn team_store() -> TeamStore {
    TeamStore::from_config(app_config())
}
fn graphs() -> GraphStore {
    GraphStore::from_config(app_config())
}
fn imports() -> ImportStore {
    ImportStore::from_config(app_config())
}
fn sources() -> SourceStore {
    SourceStore::from_config(app_config())
}
fn reads() -> ReadStore {
    ReadStore::from_config(app_config())
}
async fn admin_client() -> tokio_postgres::Client {
    let (client, connection) = admin_config()
        .connect(tokio_postgres::NoTls)
        .await
        .expect("admin connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}
async fn app_client() -> tokio_postgres::Client {
    let (client, connection) = app_config()
        .connect(tokio_postgres::NoTls)
        .await
        .expect("app connect");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    client
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let op = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    let result = match op {
        "claim" => {
            // claim <project> <work> <actor> <client> <conv>
            let (p, w, a, c, v) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
            );
            let leases = leases();
            let r = async {
                let session = leases
                    .start_session(TENANT, &p, &a, &c, &v, "main", &w)
                    .await
                    .map_err(|e| e.to_string())?;
                leases
                    .claim(TENANT, &p, &session.id, &a, &c, &format!("{v}-claim"), 600)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|cl| (session, cl))
            }
            .await;
            match r {
                Ok((s, cl)) => Ok(
                    json!({"ok":true,"op":"claim","actor":a,"work":w,"session":s.id,"claim":cl.id,"fence":cl.fence.to_string()}),
                ),
                Err(e) => Err(e),
            }
        }
        "claim-with-session" => {
            // claim-with-session <project> <work> <session> <actor> <client> <conv>
            let (p, w, s, a, c, v) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
                arg(&args, 7),
            );
            let leases = leases();
            match leases
                .claim(TENANT, &p, &s, &a, &c, &format!("{v}-claim"), 600)
                .await
            {
                Ok(cl) => Err(format!("hijack unexpectedly succeeded: claim {}", cl.id)),
                Err(e) => Ok(
                    json!({"ok":true,"op":"claim-with-session","actor":a,"work":w,"rejected":e.to_string()}),
                ),
            }
        }
        "release" => {
            // release <project> <claim> <actor>
            let (p, cl, a) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let leases = leases();
            match leases.release(TENANT, &p, &cl, &a).await {
                Ok(()) => {
                    Ok(json!({"ok":true,"op":"release","actor":a,"claim":cl,"released":true}))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        "handoff" => {
            // handoff <project> <claim> <from> <to> <to_client> <to_conv>
            let (p, cl, f, t, c, v) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
                arg(&args, 7),
            );
            let leases = leases();
            match leases.handoff(TENANT, &p, &cl, &f, &t, &c, &v).await {
                Ok(h) => Ok(
                    json!({"ok":true,"op":"handoff","from":f,"to":t,"claim":h.id,"fence":h.fence.to_string(),"session":h.session_id}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "check-fence" => {
            // check-fence <project> <work> <actor> <fence>
            let (p, w, a, f) = (arg(&args, 2), arg(&args, 3), arg(&args, 4), arg(&args, 5));
            let fence: i64 = f.parse().expect("fence int");
            let leases = leases();
            match leases
                .require_fence(TENANT, &p, "main", &w, &a, fence)
                .await
            {
                Ok(()) => Ok(
                    json!({"ok":true,"op":"check-fence","actor":a,"fence":fence.to_string(),"accepted":true}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "evidence" => {
            // evidence <project> <work> <actor> <hash> <summary> [bytes] [dirty] [input] [execution] [result_digest]
            let (p, w, a, h, s) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
            );
            let bytes = args
                .get(7)
                .filter(|b| b.as_str() != "NONE")
                .map(|b| b.clone().into_bytes());
            let dirty = args.get(8).map(|v| v == "true").unwrap_or(false);
            // Input digest binding: an explicit string is used as-is;
            // NONE/absent means no input binding (CR #59 P2-5).
            let input: Option<String> = args.get(9).filter(|v| v.as_str() != "NONE").cloned();
            // Optional execution binding for the strict completion policy
            // (CR #59 P2-5).
            let execution: Option<String> = args.get(10).filter(|e| e.as_str() != "NONE").cloned();
            // Optional declared execution RESULT digest (payload
            // "output_digest") — the strict gate requires it to match the
            // bound execution's recorded result digest; it is a different
            // contract from the artifact bytes digest (CR #59 r3 P2-1/P2-2).
            let result_digest: Option<String> =
                args.get(11).filter(|d| d.as_str() != "NONE").cloned();
            let mut payload = json!({"log": s});
            if let Some(digest) = &result_digest {
                payload
                    .as_object_mut()
                    .map(|map| map.insert("output_digest".into(), json!(digest)));
            }
            let review = reviews();
            match review
                .record_evidence(
                    TENANT,
                    &p,
                    &a,
                    &w,
                    &h,
                    None,
                    &payload,
                    bytes.as_deref(),
                    input.as_deref(),
                    dirty,
                    execution.as_deref(),
                )
                .await
            {
                Ok(ev) => Ok(
                    json!({"ok":true,"op":"evidence","actor":a,"work":w,"evidence":ev.id,"trust":ev.trust_basis}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "open-review" => {
            // open-review <project> <work> <actor> <evidence>
            let (p, w, a, ev) = (arg(&args, 2), arg(&args, 3), arg(&args, 4), arg(&args, 5));
            let review = reviews();
            match review.open_review(TENANT, &p, &a, &w, &ev).await {
                Ok(r) => Ok(json!({"ok":true,"op":"open-review","actor":a,"work":w,"round":r.id})),
                Err(e) => Err(e.to_string()),
            }
        }
        "decide" => {
            // decide <project> <round> <actor> <decision> <note>
            let (p, r, a, d, n) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
            );
            let review = reviews();
            match review.decide_review(TENANT, &p, &a, &r, &d, &n).await {
                Ok(_) => Ok(json!({"ok":true,"op":"decide","actor":a,"round":r,"decision":d})),
                Err(e) => Err(e.to_string()),
            }
        }
        "complete" => {
            // complete <project> <work> <actor> <evidence> [policy] [context_complete]
            let (p, w, a, ev) = (arg(&args, 2), arg(&args, 3), arg(&args, 4), arg(&args, 5));
            let policy = args.get(6).cloned();
            let ctx = args.get(7).map(|v| v == "true").unwrap_or(true);
            let review = reviews();
            // A stable per-intent request id: same intent retries replay,
            // new intents use new ids (CR #59 P2-4).
            let request_id = args
                .get(8)
                .cloned()
                .unwrap_or_else(|| format!("complete-{ev}"));
            match review
                .complete(
                    TENANT,
                    &p,
                    &a,
                    "cli",
                    &request_id,
                    &w,
                    "main",
                    &ev,
                    policy.as_deref(),
                    ctx,
                )
                .await
            {
                Ok(r) => Ok(json!({"ok":true,"op":"complete","actor":a,"work":w,"receipt":r.id})),
                Err(e) => Err(e.to_string()),
            }
        }
        "prepare" => {
            // prepare <project> <work> <actor> <client> <req> <claim> <hash> <declared_csv> <writes_json>
            let (p, _w, a, c, r, cl, h) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
                arg(&args, 7),
                arg(&args, 8),
            );
            let declared: Vec<String> = arg(&args, 9)
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let writes: Value = serde_json::from_str(&arg(&args, 10)).expect("writes json");
            let exec = executions();
            match exec
                .prepare(
                    TENANT,
                    &p,
                    &a,
                    &c,
                    &r,
                    &cl,
                    "runner-t13",
                    &h,
                    "in-t13",
                    "hard_fence",
                    &declared,
                    &writes,
                )
                .await
            {
                Ok(e) => Ok(
                    json!({"ok":true,"op":"prepare","actor":a,"execution":e.id,"replayed":e.replayed,"effect_key":e.effect_key}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "dispatch-twice" => {
            // dispatch-twice <project>
            let p = arg(&args, 2);
            let exec = executions();
            (async {
                let first = exec.claim_dispatch(TENANT, &p).await.map_err(|e| e.to_string())?;
                let second = exec.claim_dispatch(TENANT, &p).await.map_err(|e| e.to_string())?;
                let second_none = second.is_none();
                Ok(json!({"ok":true,"op":"dispatch-twice","first": first.map(|d| d.outbox_id), "second": second.map(|d| d.outbox_id), "redispatch_blocked": second_none}))
            }).await
        }
        "cross-read" => {
            // cross-read <project> <other_project>
            let (p, other) = (arg(&args, 2), arg(&args, 3));
            (async {
                let mut client = app_client().await;
                let tx = client.transaction().await.map_err(|e| e.to_string())?;
                tx.batch_execute(&format!("SELECT set_config('awr.tenant_id','{TENANT}',false); SELECT set_config('awr.project_id','{p}',false);")).await.map_err(|e| e.to_string())?;
                let n: i64 = tx.query_one("SELECT count(*) FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2", &[&TENANT, &other]).await.map_err(|e| e.to_string())?.get(0);
                let _ = tx.commit().await;
                Ok(json!({"ok":true,"op":"cross-read","bound_project":p,"read_project":other,"visible_rows":n,"isolated":n==0}))
            }).await
        }
        "cross-write" => {
            // cross-write <project> <other_project>
            let (p, other) = (arg(&args, 2), arg(&args, 3));
            (async {
                let mut client = app_client().await;
                let tx = client.transaction().await.map_err(|e| e.to_string())?;
                tx.batch_execute(&format!("SELECT set_config('awr.tenant_id','{TENANT}',false); SELECT set_config('awr.project_id','{p}',false);")).await.map_err(|e| e.to_string())?;
                let r = tx.execute("INSERT INTO awr_team.work_runtime(tenant_id, project_id, scope_id, work_id, state, work_version, last_fence) VALUES ($1,$2,'main','work-x','active',1,0)", &[&TENANT, &other]).await;
                let _ = tx.rollback().await;
                match r {
                    Ok(_) => Err("cross-write unexpectedly succeeded".into()),
                    Err(e) => Ok(json!({"ok":true,"op":"cross-write","bound_project":p,"write_project":other,"rejected":e.to_string()})),
                }
            }).await
        }
        "exec-cmd" => {
            // exec-cmd <project> <actor> <client> <req> <op> <argsjson>
            let (p, a, c, r, o, j) = (
                arg(&args, 2),
                arg(&args, 3),
                arg(&args, 4),
                arg(&args, 5),
                arg(&args, 6),
                arg(&args, 7),
            );
            let store = team_store();
            let req = awr_team_pg::CommandRequest {
                tenant_id: TENANT.into(),
                project_id: p.clone(),
                actor_id: a.clone(),
                client_id: c.clone(),
                request_id: r.clone(),
                op: o.clone(),
                args: serde_json::from_str(&j).expect("args json"),
            };
            match store.execute(req).await {
                Ok(out) => Ok(
                    json!({"ok":true,"op":"exec-cmd","actor":a,"request_id":r,"replayed":out.replayed,"revision":out.committed_project_revision}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "report-out-of-scope" => {
            // report-out-of-scope <project> <execution> <fence>
            let (p, e, f) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let fence: i64 = f.parse().expect("fence int");
            let exec = executions();
            (async {
                exec.accept(TENANT, &p, &e, fence).await.map_err(|e| e.to_string())?;
                exec.start(TENANT, &p, &e, fence).await.map_err(|e| e.to_string())?;
                match exec.report(TENANT, &p, "runner-t13", "trusted_executor", &e, "succeeded",
                    json!({"output_digest":"out-x"}), &["src/bar".to_string()]).await {
                    Ok(_) => Err("out-of-scope report unexpectedly succeeded".to_string()),
                    Err(err) => Ok(json!({"ok":true,"op":"report-out-of-scope","execution":e,"rejected":err.to_string()})),
                }
            }).await
        }
        "report" => {
            // report <project> <execution> <outcome> <paths_csv> [output_digest]
            let (p, e, o, paths) = (arg(&args, 2), arg(&args, 3), arg(&args, 4), arg(&args, 5));
            let explicit_digest = args.get(6).cloned();
            let observed: Vec<String> = if paths.is_empty() {
                vec![]
            } else {
                paths.split(',').map(|s| s.trim().to_string()).collect()
            };
            let exec = executions();
            let digest = explicit_digest.unwrap_or_else(|| format!("out-{o}"));
            match exec
                .report(
                    TENANT,
                    &p,
                    "runner-t13",
                    "trusted_executor",
                    &e,
                    &o,
                    json!({"output_digest": digest}),
                    &observed,
                )
                .await
            {
                Ok(r) => {
                    Ok(json!({"ok":true,"op":"report","execution":e,"outcome":o,"state":r.state}))
                }
                Err(err) => Err(err.to_string()),
            }
        }
        "graph-cycle" => {
            // graph-cycle <project> <snapshot>
            let (p, snap) = (arg(&args, 2), arg(&args, 3));
            let g = graphs();
            let edges = vec![
                DependencyEdge {
                    from: "work-a".into(),
                    to: "work-b".into(),
                    relation: "requires".into(),
                    required: true,
                },
                DependencyEdge {
                    from: "work-b".into(),
                    to: "work-a".into(),
                    relation: "requires".into(),
                    required: true,
                },
            ];
            match g
                .replace_edges(
                    TENANT,
                    &p,
                    &snap,
                    "main",
                    &["work-a".into(), "work-b".into()],
                    &edges,
                )
                .await
            {
                Ok(()) => Err("cycle graph unexpectedly accepted".into()),
                Err(e) => Ok(json!({"ok":true,"op":"graph-cycle","rejected":e.to_string()})),
            }
        }
        "reserve" => {
            // reserve <project> <work> <kind> <path>
            let (p, w, k, path) = (arg(&args, 2), arg(&args, 3), arg(&args, 4), arg(&args, 5));
            let g = graphs();
            match g.reserve(TENANT, &p, &w, &k, &path).await {
                Ok(rid) => {
                    Ok(json!({"ok":true,"op":"reserve","work":w,"path":path,"reservation":rid}))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        "split" => {
            // split <project> <work> <child1,child2>
            let (p, w, kids) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let children: Vec<String> = kids.split(',').map(|s| s.trim().to_string()).collect();
            let g = graphs();
            match g
                .propose_split(
                    TENANT,
                    &p,
                    &w,
                    &children,
                    &json!({"acceptance":"inherited"}),
                )
                .await
            {
                Ok(s) => Ok(json!({"ok":true,"op":"split","work":w,"children":s.child_work_ids})),
                Err(e) => Err(e.to_string()),
            }
        }
        "complete-parent" => {
            // complete-parent <project> <work>
            let (p, w) = (arg(&args, 2), arg(&args, 3));
            let g = graphs();
            match g.complete_parent_from_children(TENANT, &p, &w).await {
                Ok(_) => Err("parent completed from children unexpectedly".into()),
                Err(e) => {
                    Ok(json!({"ok":true,"op":"complete-parent","work":w,"rejected":e.to_string()}))
                }
            }
        }
        "bind-invalidate" => {
            // bind-invalidate <project> <work> <upstream>
            let (p, w, up) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let g = graphs();
            (async {
                g.bind_dependency(TENANT, &p, &w, &up, "bind-live").await.map_err(|e| e.to_string())?;
                g.invalidate_downstream(TENANT, &p, &up).await.map_err(|e| e.to_string())?;
                let valid = g.current_binding_valid(TENANT, &p, &w, &up).await.map_err(|e| e.to_string())?;
                Ok(json!({"ok":true,"op":"bind-invalidate","work":w,"upstream":up,"binding_valid_after_invalidation":valid}))
            }).await
        }
        "activate-check" => {
            // activate-check <project> <work> <hash>
            let (p, w, h) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let g = graphs();
            match g.activation_blocked_by_claims(TENANT, &p, &w, &h).await {
                Ok(()) => {
                    Ok(json!({"ok":true,"op":"activate-check","work":w,"hash":h,"allowed":true}))
                }
                Err(e) => Err(e.to_string()),
            }
        }
        "graph-scope" => {
            // graph-scope <project> <snapshot> <scope>
            let (p, snap, s) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let g = graphs();
            match g
                .replace_edges(TENANT, &p, &snap, &s, &["work-a".into()], &[])
                .await
            {
                Ok(()) => Err("unknown scope unexpectedly accepted".into()),
                Err(e) => {
                    Ok(json!({"ok":true,"op":"graph-scope","scope":s,"rejected":e.to_string()}))
                }
            }
        }
        "budget-check" => {
            let g = graphs();
            let edges = vec![
                DependencyEdge {
                    from: "a".into(),
                    to: "b".into(),
                    relation: "requires".into(),
                    required: true,
                },
                DependencyEdge {
                    from: "b".into(),
                    to: "c".into(),
                    relation: "requires".into(),
                    required: true,
                },
                DependencyEdge {
                    from: "c".into(),
                    to: "d".into(),
                    relation: "requires".into(),
                    required: true,
                },
            ];
            match g.graph_within_budget(&edges, 2).await {
                Ok(()) => Err("over-budget graph unexpectedly accepted".into()),
                Err(e) => Ok(
                    json!({"ok":true,"op":"budget-check","edges":3,"budget":2,"rejected":e.to_string()}),
                ),
            }
        }
        "freeze" => {
            let p = arg(&args, 2);
            let imp = imports();
            match imp.freeze(TENANT, &p).await {
                Ok(()) => Ok(json!({"ok":true,"op":"freeze","project":p})),
                Err(e) => Err(e.to_string()),
            }
        }
        "import-load" => {
            // import-load <project> <actor> <key>
            let (p, a, k) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let imp = imports();
            // Run freeze first. Imports require explicit contracts and material
            // identities; claimed historical trust does not grant authority.
            let manifest = json!({"format":"awr-team-import-v1","scopes":["main"],"works":[{"id":"work-i1","external_key":"IMP-1","contract":{"codec":"awr-team-contract-v1","work_id":"work-i1","external_key":"IMP-1","goals":[],"hard_rules":[],"scope_paths":[],"acceptance":["verify imported work"],"required_dependencies":[],"completion_policy":"ordinary_confirm","verification_requirements":[]}}],"evidence":[]});
            match imp.load(TENANT, &p, &a, &k, &manifest).await {
                Ok(j) => Ok(
                    json!({"ok":true,"op":"import-load","actor":a,"job":j.id,"replayed":j.replayed,"state":j.state}),
                ),
                Err(e) => Err(e.to_string()),
            }
        }
        "inspect-sources" => {
            // inspect-sources <fp1> <fp2>
            let (f1, f2) = (arg(&args, 2), arg(&args, 3));
            let imp = ImportStore::new("postgres://unused");
            match imp.inspect_sources(&[("a", &f1), ("b", &f2)]) {
                Ok(r) => Ok(json!({"ok":true,"op":"inspect-sources","diverged":r.diverged})),
                Err(e) => Err(e.to_string()),
            }
        }
        "ingest-unsafe" => {
            // ingest-unsafe <project> <actor>
            let (p, a) = (arg(&args, 2), arg(&args, 3));
            let src = sources();
            let req = IngestRequest {
                tenant_id: TENANT.into(),
                project_id: p.clone(),
                actor_id: a.clone(),
                parser_version: "t13".into(),
                files: vec![SourceFile {
                    path: "../evil/contract.yaml".into(),
                    bytes: b"evil".to_vec(),
                }],
            };
            match src.ingest(req).await {
                Ok(_) => Err("unsafe path unexpectedly ingested".into()),
                Err(e) => {
                    Ok(json!({"ok":true,"op":"ingest-unsafe","actor":a,"rejected":e.to_string()}))
                }
            }
        }
        "canonical-hash" => {
            let a = json!({"op":"work.claim","args":{"work_id":"w1","scope":"main"},"actor":"kimi-cli"});
            let b = json!({"actor":"kimi-cli","args":{"scope":"main","work_id":"w1"},"op":"work.claim"});
            match (awr_team::request_hash(&a), awr_team::request_hash(&b)) {
                (Ok(ha), Ok(hb)) => Ok(
                    json!({"ok":true,"op":"canonical-hash","hash_a":ha,"hash_b":hb,"equal":ha==hb}),
                ),
                _ => Err("hash failed".to_string()),
            }
        }
        "events-page" => {
            // events-page <project> <limit>
            let (p, l) = (arg(&args, 2), arg(&args, 3));
            let limit: i64 = l.parse().expect("limit int");
            let read = reads();
            (async {
                let mut cursor: Option<String> = None;
                let mut pages = 0u32;
                let mut total = 0usize;
                let mut seen = std::collections::HashSet::new();
                let mut dup = false;
                loop {
                    let page = read.list_events(TENANT, &p, cursor.as_deref(), limit).await.map_err(|e| e.to_string())?;
                    pages += 1;
                    total += page.events.len();
                    if page.events.is_empty() { break; }
                    for e in &page.events {
                        if !seen.insert(e.id.clone()) { dup = true; }
                    }
                    if page.next_cursor.is_empty() { break; }
                    cursor = Some(page.next_cursor);
                    if pages > 100 { break; }
                }
                Ok(json!({"ok":true,"op":"events-page","project":p,"pages":pages,"total":total,"duplicates":dup,"last_cursor":cursor}))
            }).await
        }
        "three-surfaces" => three_surfaces(),
        "complete-direct" => {
            let (p, w, a) = (arg(&args, 2), arg(&args, 3), arg(&args, 4));
            let review = reviews();
            match review
                .complete(
                    TENANT,
                    &p,
                    &a,
                    "cli",
                    "cli-direct",
                    &w,
                    "main",
                    "ev-nonexistent",
                    None,
                    true,
                )
                .await
            {
                Ok(_) => Err("direct complete unexpectedly succeeded".into()),
                Err(e) => {
                    Ok(json!({"ok":true,"op":"complete-direct","actor":a,"rejected":e.to_string()}))
                }
            }
        }
        "oracle" => {
            let (p, w) = (arg(&args, 2), arg(&args, 3));
            oracle(&p, &w).await
        }
        other => Err(format!("unknown op {other}")),
    };
    match result {
        Ok(v) => println!("{v}"),
        Err(e) => {
            println!("{}", json!({"ok":false,"error":e}));
            std::process::exit(1);
        }
    }
}

fn three_surfaces() -> Result<Value, String> {
    let auth = awr_team::AuthContext {
        tenant_id: TENANT.into(),
        project_id: "project-tc003".into(),
        actor_id: "kimi-cli".into(),
        client_id: "kimi".into(),
    };
    let remote = awr_team::RemoteProfile {
        name: "t13".into(),
        endpoint: "http://127.0.0.1/team/v1".into(),
        project_key: "tc003".into(),
        credential_env: "AWR_TEAM_TOKEN".into(),
        protocol_version: 1,
    };
    let env = awr_team::parse_envelope(&json!({"protocol_version":1,"request_id":"s1","op":"work.claim","args":{"work_id":"work-tc003","scope_id":"main","session_id":"s1","expected_work_version":"1","expected_contract_hash":"hash-tc003"}})).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for surface in ["http", "mcp", "cli"] {
        let offline_err = awr_team::execute(surface, env.clone(), &auth, None, true)
            .unwrap_err()
            .code()
            .to_string();
        let accepted = awr_team::execute(surface, env.clone(), &auth, Some(&remote), true)
            .map_err(|e| e.to_string())?;
        results.push(json!({"surface":surface,"offline_error":offline_err,"accepted":accepted}));
    }
    let offline_consistent = results[0]["offline_error"] == results[1]["offline_error"]
        && results[1]["offline_error"] == results[2]["offline_error"];
    let acc: Vec<Value> = results
        .iter()
        .map(|r| {
            let mut a = r["accepted"].clone();
            a.as_object_mut().map(|m| m.remove("surface"));
            a
        })
        .collect();
    let accept_consistent = acc[0] == acc[1] && acc[1] == acc[2];
    Ok(
        json!({"ok":true,"op":"three-surfaces","results":results,"offline_consistent":offline_consistent,"accept_consistent":accept_consistent}),
    )
}

fn arg(args: &[String], i: usize) -> String {
    args.get(i)
        .unwrap_or_else(|| panic!("missing arg {i}"))
        .clone()
}

async fn oracle(project: &str, work: &str) -> Result<Value, String> {
    let (client, connection) = admin_config()
        .connect(tokio_postgres::NoTls)
        .await
        .map_err(|e| e.to_string())?;
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let active: i64 = client.query_one(
        "SELECT count(*) FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='active'",
        &[&TENANT, &project, &work]).await.map_err(|e| e.to_string())?.get(0);
    let holders: Vec<String> = client.query(
        "SELECT actor_id FROM awr_team.claims WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='active'",
        &[&TENANT, &project, &work]).await.map_err(|e| e.to_string())?.iter().map(|r| r.get(0)).collect();
    let sessions: i64 = client.query_one(
        "SELECT count(*) FROM awr_team.sessions WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3",
        &[&TENANT, &project, &work]).await.map_err(|e| e.to_string())?.get(0);
    let receipts: i64 = client.query_one(
        "SELECT count(*) FROM awr_team.completion_receipts WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3",
        &[&TENANT, &project, &work]).await.map_err(|e| e.to_string())?.get(0);
    Ok(
        json!({"project":project,"work":work,"active_claims":active,"holders":holders,"sessions":sessions,"receipts":receipts}),
    )
}
