// Included in agent_review_tests; isolated PG fixture, not native acceptance.
async fn on_work(store: &WorkstreamReadStore, token: &str, work: &str, key: &str, op: &str, args: Value) -> Value {
    let p = prepare(store, token, work).await;
    store.commands().execute(TENANT, PROJECT, token, command(&p, key, op, args)).await.unwrap()["receipt"]["data"].clone()
}

#[tokio::test]
async fn opted_in_consumer_executes_and_completes_with_exact_agent_predecessor() {
    let (_g, admin, db, store) = setup().await;
    let chain = caller_chain(&admin, &db, &store).await;
    let upstream = run(&store, RUNNER, "opt-upstream-complete", "work.complete", completion_args(&chain)).await;
    let mut consumer: WorkContract = serde_json::from_value(admin.query_one(
        "SELECT contract_json FROM awr_team.work_contracts WHERE work_id='c'", &[]).await.unwrap().get(0)).unwrap();
    consumer.codec = WorkContract::CODEC_V2.into();
    consumer.completion_policy = POLICY.into();
    consumer.required_dependencies = vec!["a".into()];
    consumer.dependency_acceptance.insert("a".into(), awr_team::DependencyAcceptanceMode::AgentReviewedCallerAssertedReconciled);
    admin.execute("UPDATE awr_team.work_contracts SET contract_json=$1,contract_hash=$2 WHERE work_id='c'",
        &[&json!(consumer),&consumer.hash().unwrap()]).await.unwrap();
    admin.batch_execute("INSERT INTO awr_team.sessions(tenant_id,project_id,id,scope_id,work_id,actor_id,client_id,conversation_id,state,workstream_id,ownership_version)
        SELECT tenant_id,project_id,id||'-c',scope_id,'c',actor_id,client_id,id||'-c','active',workstream_id,ownership_version
        FROM awr_team.sessions WHERE id IN ('session-a','session-runner','session-reviewer')").await.unwrap();
    let next = store.query(TENANT,PROJECT,A,query("work.next")).await.unwrap();
    let candidate = next["data"]["items"].as_array().unwrap().iter().find(|x|x["work_id"]=="c").unwrap();
    assert_ne!(candidate["navigation"], "waiting_dependency");
    let claim = on_work(&store,A,"c","opt-take","claim.acquire",json!({"session_id":"session-a-c","expected_session_version":"1","expected_work_version":"0","ttl_seconds":600})).await;
    let p=prepare(&store,A,"c").await;
    let intent=on_work(&store,A,"c","opt-intent","execution.prepare",json!({"session_id":"session-a-c","expected_session_version":"1",
        "claim_id":claim["claim_id"],"expected_fence":claim["fence"],"expected_lease_version":claim["lease_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],"input_digest":INPUT,"declared_scope":["src/integration"]})).await;
    let p=prepare(&store,A,"c").await;
    let started=on_work(&store,A,"c","opt-start","execution.start",json!({"session_id":"session-a-c","expected_session_version":"1",
        "claim_id":claim["claim_id"],"expected_fence":claim["fence"],"expected_lease_version":claim["lease_version"],
        "execution_id":intent["execution_id"],"expected_execution_version":intent["execution_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],"execution_mode":"caller_managed"})).await;
    assert_eq!(started["state"],"running");
    let reported=on_work(&store,A,"c","opt-report","execution.report",json!({"session_id":"session-a-c","expected_session_version":"1",
        "execution_id":intent["execution_id"],"expected_execution_version":started["execution_version"],"outcome":"succeeded",
        "output_digest":RESULT,"observed_paths":["src/integration/result.json"],"note":"Caller observed integration result."})).await;
    let ev=on_work(&store,A,"c","opt-evidence","evidence.submit",submit_args("session-a-c",intent["execution_id"].as_str().unwrap(),&hex_encode(b"consumer artifact"))).await;
    let opened=on_work(&store,A,"c","opt-open","review.open",json!({"session_id":"session-a-c","expected_session_version":"1","evidence_id":ev["evidence_id"]})).await;
    let mut decision=args(&opened); decision["session_id"]=json!("session-reviewer-c");
    on_work(&store,REVIEWER_TOKEN,"c","opt-review","review.decide",decision).await;
    let p=prepare(&store,RUNNER,"c").await;
    on_work(&store,RUNNER,"c","opt-reconcile","execution.reconcile",json!({"session_id":"session-runner-c","expected_session_version":"1",
        "execution_id":intent["execution_id"],"expected_execution_version":reported["execution_version"],
        "expected_work_version":p["data"]["runtime"]["work_version"],"reviewed_receipt_id":reported["receipt_id"],"clear_recovery_block":true,
        "facts":{"outcome":"succeeded","input_digest":INPUT,"output_digest":RESULT,"environment_digest":"c".repeat(64),
            "observed_paths":["src/integration/result.json"],"note":"Operator reconciled caller effects."}})).await;
    let mut finalize=completion_args(&ev);finalize["session_id"]=json!("session-runner-c");
    let p=prepare(&store,RUNNER,"c").await;
    let request=command(&p,"opt-complete","work.complete",finalize.clone());
    // A receipt that is no longer selected cannot satisfy even an explicit opt-in.
    admin.batch_execute("UPDATE awr_team.work_runtime SET state='unclaimed',selected_completion_id=NULL WHERE work_id='a'").await.unwrap();
    let denied=store.commands().execute(TENANT,PROJECT,RUNNER,request).await.unwrap_err();
    assert!(matches!(denied,PgError::CompletionRejected), "unexpected: {denied:?}");
    admin.execute("UPDATE awr_team.work_runtime SET state='completed',selected_completion_id=$1 WHERE work_id='a'",
        &[&upstream["receipt_id"].as_str().unwrap()]).await.unwrap();
    let completed=on_work(&store,RUNNER,"c","opt-complete-after-restore","work.complete",finalize).await;
    assert_eq!(completed["human_approval"],false);
    let rows=admin.query("SELECT predecessor_work_id,predecessor_completion_id FROM awr_team.completion_dependencies WHERE completion_id=$1",
        &[&completed["receipt_id"].as_str().unwrap()]).await.unwrap();
    assert_eq!(rows.len(),1);
    assert_eq!(rows[0].get::<_,String>(0),"a");
    assert_eq!(rows[0].get::<_,String>(1),upstream["receipt_id"].as_str().unwrap());
}
