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
    let facts = f.facts().await;
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
async fn disconnect_expires_delivery_but_does_not_claim_native_wait_ended() {
    let f = Fixture::with_native_resolution(false).await;
    f.send(request("applyPatchApproval", json!(1)));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    let facts = vec![("interaction.blocked".into(), interaction.interaction_id)];
    f.wait_facts(&facts).await;
    f.send(Value::Null);
    f.wait(1, InteractionState::Expired).await;
    assert_eq!(f.facts().await, facts);
    f.assert_state("waiting_for_interaction").await;
    f.stop().await;
}
