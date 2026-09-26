use super::*;
use serde_json::{Value, json};

#[tokio::test]
async fn policy_removal_reopens_only_affected_completions_atomically_and_keeps_history() {
    let (_g, admin, _, store) = setup().await;
    let mut original = bundle();
    original.codec = WorkstreamBundle::CODEC_V2.into();
    original.contracts[1].workstream_id = original.contracts[0].workstream_id;
    original.contracts[1].contract.codec = WorkContract::CODEC_V2.into();
    original.contracts[1].contract.dependency_acceptance.insert(
        "interface".into(),
        awr_team::DependencyAcceptanceMode::AgentReviewedCallerAssertedReconciled,
    );
    let mut independent = original.contracts[0].clone();
    independent.contract.work_id = WorkId::new("unrelated").unwrap();
    independent.contract.external_key = "unrelated".into();
    original.contracts.push(independent);
    let first = approved(&store, package(&original)).await;
    store
        .activate_workstreams(TENANT, PROJECT, "author", &first.proposal_id, &plan(&first))
        .await
        .unwrap();
    admin
        .batch_execute(
            "INSERT INTO awr_team.work_scopes(tenant_id,project_id,id,name,status)
        VALUES('ws-source-tenant','ws-source-project','history','History','active')",
        )
        .await
        .unwrap();
    for entry in &original.contracts {
        let work = entry.contract.work_id.as_str();
        for scope in ["main", "history"] {
            let receipt = format!("{scope}-{work}");
            admin.execute("INSERT INTO awr_team.completion_receipts(tenant_id,project_id,id,work_id,scope_id,contract_hash,
                result_digest,dependency_binding_hash,evidence_bundle_hash,policy,approved_by_json)
                VALUES($1,$2,$3,$4,$5,$6,'fixture-result','fixture-binding','fixture-evidence','review','{}')",
                &[&TENANT,&PROJECT,&receipt,&work,&scope,&entry.contract.hash().unwrap()]).await.unwrap();
            admin.execute("INSERT INTO awr_team.work_runtime(tenant_id,project_id,scope_id,work_id,state,selected_completion_id,work_version)
                VALUES($1,$2,$3,$4,'completed',$5,6)", &[&TENANT,&PROJECT,&scope,&work,&receipt]).await.unwrap();
        }
    }
    for (consumer, predecessor) in [("sdk", "interface"), ("integration", "sdk")] {
        admin.execute("INSERT INTO awr_team.completion_dependencies(tenant_id,project_id,completion_id,predecessor_work_id,predecessor_completion_id)
            VALUES($1,$2,$3,$4,$5)", &[&TENANT,&PROJECT,&format!("main-{consumer}"),&predecessor,&format!("main-{predecessor}")]).await.unwrap();
    }
    // An unrelated source edit preserves every current receipt.
    let mut renamed = original.clone();
    renamed.catalog.workstreams[0].title = "Renamed stream".into();
    let rename = approved(&store, package(&renamed)).await;
    store
        .activate_workstreams(
            TENANT,
            PROJECT,
            "author",
            &rename.proposal_id,
            &plan(&rename),
        )
        .await
        .unwrap();
    let count: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.work_runtime WHERE state='completed'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 8);

    let mut tightened = renamed;
    tightened.codec = WorkstreamBundle::CODEC.into();
    tightened.contracts[1].contract.codec = WorkContract::CODEC.into();
    tightened.contracts[1]
        .contract
        .dependency_acceptance
        .clear();
    let candidate = approved(&store, package(&tightened)).await;
    // Fail AFTER invalidation, so a rollback proves selection and source change atomicity.
    admin.batch_execute("CREATE FUNCTION fail_source_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
        IF NEW.event_type='source.activated' THEN RAISE EXCEPTION 'injected source event failure'; END IF; RETURN NEW; END $$;
        CREATE TRIGGER fail_source_event BEFORE INSERT ON awr_team.events FOR EACH ROW EXECUTE FUNCTION fail_source_event();").await.unwrap();
    assert!(
        store
            .activate_workstreams(
                TENANT,
                PROJECT,
                "author",
                &candidate.proposal_id,
                &plan(&candidate)
            )
            .await
            .is_err()
    );
    assert_eq!(current_snapshot(&admin).await, Some(rename.snapshot_id));
    let count: i64 = admin
        .query_one(
            "SELECT count(*) FROM awr_team.work_runtime WHERE state='completed' AND work_version=6",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 8);
    admin
        .batch_execute(
            "DROP TRIGGER fail_source_event ON awr_team.events; DROP FUNCTION fail_source_event();",
        )
        .await
        .unwrap();
    store
        .activate_workstreams(
            TENANT,
            PROJECT,
            "author",
            &candidate.proposal_id,
            &plan(&candidate),
        )
        .await
        .unwrap();
    for row in admin.query("SELECT scope_id,work_id,state,selected_completion_id,work_version FROM awr_team.work_runtime",&[]).await.unwrap() {
        let scope: String=row.get(0); let work: String=row.get(1);
        let stale=scope=="main" && (work=="sdk" || work=="integration");
        assert_eq!(row.get::<_,String>(2),if stale {"unclaimed"} else {"completed"});
        assert_eq!(row.get::<_,Option<String>>(3),if stale {None} else {Some(format!("{scope}-{work}"))});
        assert_eq!(row.get::<_,i64>(4),if stale {7} else {6});
    }
    let history: i64 = admin
        .query_one("SELECT count(*) FROM awr_team.completion_receipts", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(history, 8);
    let event: Value=admin.query_one("SELECT payload_json FROM awr_team.events WHERE event_type='source.activated' ORDER BY project_revision DESC LIMIT 1",&[]).await.unwrap().get(0);
    let mut affected = event["completion_invalidations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["work_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    affected.sort();
    assert_eq!(affected, vec!["integration", "sdk"]);
    assert_eq!(event["snapshot_id"], json!(candidate.snapshot_id));
}

#[tokio::test]
async fn standalone_legacy_source_rejects_unscoped_v2_dependency_policy() {
    let (_g, _admin, _, store) = setup().await;
    let mut request = legacy_package();
    let mut c = bundle().contracts.remove(0).contract;
    c.codec = WorkContract::CODEC_V2.into();
    c.required_dependencies.push("upstream".into());
    c.dependency_acceptance.insert(
        "upstream".into(),
        awr_team::DependencyAcceptanceMode::AgentReviewedCallerAssertedReconciled,
    );
    request.files[0].bytes = serde_json::to_vec(&c).unwrap();
    assert!(matches!(
        store.ingest(request).await,
        Err(PgError::Unsupported(_))
    ));
}
