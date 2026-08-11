use super::validation::valid_permission_suggestion;
use super::*;
use serde_json::json;

#[test]
fn suggestion_validation_preserves_only_exact_bounded_schema() {
    let valid = json!({
        "type": "addRules",
        "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
        "behavior": "allow",
        "destination": "localSettings"
    });
    assert!(valid_permission_suggestion(&valid));

    let mut unknown = valid.clone();
    unknown["unexpected"] = json!(true);
    assert!(!valid_permission_suggestion(&unknown));

    let oversized = json!({
        "type": "addRules",
        "rules": [{"toolName": "Bash", "ruleContent": "x".repeat(MAX_APPROVAL_STRING_CHARS + 1)}],
        "behavior": "allow",
        "destination": "localSettings"
    });
    assert!(!valid_permission_suggestion(&oversized));

    let too_many_rules = json!({
        "type": "addRules",
        "rules": (0..=MAX_PERMISSION_RULES)
            .map(|index| json!({"toolName": "Bash", "ruleContent": format!("command {index}")}))
            .collect::<Vec<_>>(),
        "behavior": "allow",
        "destination": "localSettings"
    });
    assert!(!valid_permission_suggestion(&too_many_rules));
}

#[tokio::test]
async fn concurrent_decisions_only_wake_a_waiter_once() {
    let coordinator = ApprovalCoordinator::default();
    let receiver = coordinator
        .register(
            "evt_approval".to_string(),
            "sess_approval".to_string(),
            "turn_approval".to_string(),
            json!({}),
            Vec::new(),
        )
        .await
        .unwrap();

    let accept = coordinator.deliver_decision(
        "evt_approval",
        "sess_approval",
        ApprovalWaitOutcome::AcceptOnce,
    );
    let reject =
        coordinator.deliver_decision("evt_approval", "sess_approval", ApprovalWaitOutcome::Reject);
    let (accept, reject) = tokio::join!(accept, reject);

    assert_ne!(accept.is_ok(), reject.is_ok());
    assert!(matches!(
        receiver.await.unwrap(),
        ApprovalWaitOutcome::AcceptOnce | ApprovalWaitOutcome::Reject
    ));
}
