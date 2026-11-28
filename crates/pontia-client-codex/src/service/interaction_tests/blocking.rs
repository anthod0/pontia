use super::*;

fn resolved(thread: &str, id: Value) -> Value {
    json!({"method":"serverRequest/resolved","params":{"threadId":thread,"requestId":id}})
}

#[tokio::test]
async fn response_delivery_does_not_release_wait_before_native_resolution() {
    let mut f = Fixture::with_native_resolution(false).await;
    f.send(request("item/commandExecution/requestApproval", json!(1)));
    let first = f.wait(1, InteractionState::Pending).await.remove(0);
    let mut facts = vec![("interaction.blocked".into(), first.interaction_id.clone())];
    f.wait_facts(&facts).await;
    f.app
        .interactions()
        .respond(&f.app, &f.session, &first.interaction_id, "allow")
        .await
        .unwrap();
    f.response().await;

    f.assert_state("waiting_for_interaction").await;
    assert_eq!(f.facts().await, facts);
    assert_eq!(
        f.app
            .interactions()
            .get(&f.session, &first.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Resolved
    );
    f.send(resolved("thread", json!(1)));
    facts.push(("interaction.unblocked".into(), first.interaction_id));
    f.wait_facts(&facts).await;
    f.assert_state("busy").await;
    f.stop().await;
}

#[tokio::test]
async fn independent_waits_ignore_duplicates_output_and_wrong_resolution_identity() {
    let f = Fixture::with_native_resolution(false).await;
    let approval = request("item/fileChange/requestApproval", json!(1));
    f.send(approval.clone());
    let first = f.wait(1, InteractionState::Pending).await.remove(0);
    f.send(approval);
    f.send(resolved("other-thread", json!(1)));
    f.send(resolved("thread", json!("1")));
    f.send(json!({"method":"item/started","params":{"threadId":"thread","turnId":"native-turn","item":{"id":"parallel-command","type":"commandExecution"}}}));
    f.send(json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thread","turnId":"native-turn","delta":"parallel output"}}));
    f.send(request("item/permissions/requestApproval", json!("two")));
    let pending = f.wait(2, InteractionState::Pending).await;
    let second = pending
        .iter()
        .find(|i| i.interaction_id != first.interaction_id)
        .unwrap();
    let mut facts = vec![
        ("interaction.blocked".into(), first.interaction_id.clone()),
        ("interaction.blocked".into(), second.interaction_id.clone()),
    ];
    f.wait_facts(&facts).await;
    f.assert_state("waiting_for_interaction").await;
    f.send(resolved("thread", json!(1)));
    facts.push(("interaction.unblocked".into(), first.interaction_id.clone()));
    f.wait_facts(&facts).await;
    f.assert_state("waiting_for_interaction").await;
    f.send(resolved("thread", json!(1)));
    f.send(resolved("thread", json!("two")));
    facts.push((
        "interaction.unblocked".into(),
        second.interaction_id.clone(),
    ));
    f.wait_facts(&facts).await;
    f.assert_state("busy").await;
    f.stop().await;
}

#[tokio::test]
async fn turn_end_and_delivery_expiry_retain_correlation_without_synthesizing_release() {
    let f = Fixture::with_native_resolution(false).await;
    f.send(request("execCommandApproval", json!("old")));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    let mut facts = vec![(
        "interaction.blocked".into(),
        interaction.interaction_id.clone(),
    )];
    f.wait_facts(&facts).await;
    f.send(json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"native-turn","status":"completed","items":[]}}}));
    f.wait(1, InteractionState::Expired).await;
    assert_eq!(f.facts().await, facts);
    f.send(resolved("thread", json!("old")));
    facts.push(("interaction.unblocked".into(), interaction.interaction_id));
    f.wait_facts(&facts).await;
    // Native interaction expiry and the completed Turn are observed separately.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let completed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM turns WHERE session_id=? AND state='completed')",
            )
            .bind(&f.session)
            .fetch_one(&f.app.db())
            .await
            .unwrap();
            if completed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.assert_state("idle").await;
    f.stop().await;
}

#[tokio::test]
async fn native_resolution_is_preserved_during_a_notification_burst() {
    let f = Fixture::with_native_resolution(false).await;
    f.send(request("item/permissions/requestApproval", json!(1)));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    let mut facts = vec![(
        "interaction.blocked".into(),
        interaction.interaction_id.clone(),
    )];
    f.wait_facts(&facts).await;
    for _ in 0..8192 {
        f.send(json!({"method":"ignored/output"}));
    }
    f.send(resolved("thread", json!(1)));
    for _ in 0..8192 {
        f.send(json!({"method":"ignored/output"}));
    }
    facts.push(("interaction.unblocked".into(), interaction.interaction_id));
    f.wait_facts(&facts).await;
    f.assert_state("busy").await;
    f.stop().await;
}

#[tokio::test]
async fn approvals_resolved_immediately_still_persist_both_ordered_facts() {
    let f = Fixture::with_native_resolution(false).await;
    for id in 1..=32 {
        f.send(request("item/fileChange/requestApproval", json!(id)));
        f.send(resolved("thread", json!(id)));
    }
    let expired = f.wait(32, InteractionState::Expired).await;
    let facts = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let facts = f.facts().await;
            if facts.len() >= 64 {
                break facts;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("32 approvals must persist their blocked and unblocked facts");
    assert_eq!(facts.len(), 64);
    for interaction in expired {
        let matching: Vec<&str> = facts
            .iter()
            .filter(|(_, id)| id == &interaction.interaction_id)
            .map(|(kind, _)| kind.as_str())
            .collect();
        assert_eq!(matching, ["interaction.blocked", "interaction.unblocked"]);
    }
    f.assert_state("busy").await;
    f.stop().await;
}

#[tokio::test]
async fn late_native_resolution_does_not_reactivate_an_exited_session() {
    let f = Fixture::with_native_resolution(false).await;
    f.send(request("item/fileChange/requestApproval", json!(1)));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    let mut facts = vec![(
        "interaction.blocked".into(),
        interaction.interaction_id.clone(),
    )];
    f.wait_facts(&facts).await;
    f.app
        .session_commands()
        .request_exit(&f.session, None)
        .await
        .unwrap();
    f.send(resolved("thread", json!(1)));
    facts.push(("interaction.unblocked".into(), interaction.interaction_id));
    f.wait_facts(&facts).await;
    f.assert_state("exited").await;
    f.stop().await;
}

#[tokio::test]
async fn disconnect_preserves_delivery_during_grace_without_claiming_native_wait_ended() {
    let f = Fixture::with_native_resolution(false).await;
    f.send(request("applyPatchApproval", json!(1)));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    let facts = vec![("interaction.blocked".into(), interaction.interaction_id)];
    f.wait_facts(&facts).await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    f.send(Value::Null);
    tokio::time::timeout(Duration::from_secs(5), async {
        while connection.is_connected() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    f.wait(1, InteractionState::Pending).await;
    assert_eq!(f.facts().await, facts);
    f.assert_state("waiting_for_interaction").await;
    f.stop().await;
}

#[tokio::test]
async fn conflicting_native_registration_denies_once_and_leaves_other_requests_deliverable() {
    let mut f = Fixture::new().await;
    let initial = request("item/fileChange/requestApproval", json!("conflicting-id"));
    f.send(initial.clone());
    let first = f.wait(1, InteractionState::Pending).await.remove(0);
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let instance = runtime
        .connection()
        .await
        .unwrap()
        .identity
        .instance_id
        .clone();
    let key: String =
        sqlx::query_scalar("SELECT report_key FROM interactions WHERE interaction_id=?")
            .bind(&first.interaction_id)
            .fetch_one(&f.app.db())
            .await
            .unwrap();
    assert_eq!(key, format!("codex:{instance}:\"conflicting-id\""));
    let mut changed = initial;
    changed["params"]["grantRoot"] = json!("/a/different/root");
    f.send(changed);
    assert_eq!(
        f.response().await,
        json!({"id":"conflicting-id","result":{"decision":"decline"}})
    );
    f.wait(1, InteractionState::Expired).await;
    f.send(request(
        "item/fileChange/requestApproval",
        json!("independent"),
    ));
    let next = f.wait(1, InteractionState::Pending).await.remove(0);
    f.app
        .interactions()
        .respond(&f.app, &f.session, &next.interaction_id, "allow")
        .await
        .unwrap();
    assert_eq!(f.response().await["id"], "independent");
    assert_eq!(
        f.app
            .interactions()
            .list(&f.session, None, 10, None)
            .await
            .unwrap()
            .interactions
            .len(),
        2
    );
    f.stop().await;
}

#[tokio::test]
async fn temporary_storage_failure_leaves_independent_requests_and_delivered_choices_alone() {
    let mut f = Fixture::new().await;
    let db = f.app.db();
    sqlx::raw_sql("CREATE TABLE full_disk(id INTEGER PRIMARY KEY AUTOINCREMENT);
        INSERT INTO full_disk VALUES(9223372036854775807);
        CREATE TRIGGER fail_registration BEFORE INSERT ON interactions WHEN NEW.description = 'retry me'
        BEGIN INSERT INTO full_disk VALUES(NULL); END;").execute(&db).await.unwrap();
    let mut first = request("item/fileChange/requestApproval", json!(1));
    first["params"]["reason"] = json!("retry me");
    let mut obsolete = first.clone();
    obsolete["id"] = json!(3);
    f.send(first);
    f.send(obsolete);
    f.send(request("item/fileChange/requestApproval", json!(2)));
    let independent = f.wait(1, InteractionState::Pending).await.remove(0);
    assert_ne!(independent.description.as_deref(), Some("retry me"));
    f.send(resolved("thread", json!(3)));
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while connection.request_pending("3").await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    sqlx::query("DROP TRIGGER fail_registration")
        .execute(&db)
        .await
        .unwrap();
    f.wait(2, InteractionState::Pending).await;
    sqlx::raw_sql("CREATE TRIGGER fail_unblocked BEFORE INSERT ON events WHEN NEW.event_type = 'interaction.unblocked'
        BEGIN INSERT INTO full_disk VALUES(NULL); END;").execute(&db).await.unwrap();
    f.app
        .interactions()
        .respond(&f.app, &f.session, &independent.interaction_id, "allow")
        .await
        .unwrap();
    assert_eq!(
        f.response().await,
        json!({"id":2,"result":{"decision":"accept"}})
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(f.facts().await.len(), 2);
    let mut expected = f.facts().await;
    expected.push(("interaction.unblocked".into(), independent.interaction_id));
    sqlx::query("DROP TRIGGER fail_unblocked")
        .execute(&db)
        .await
        .unwrap();
    f.wait_facts(&expected).await;
    assert!(f.replies.try_recv().is_err());
    f.stop().await;
}
