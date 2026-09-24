use super::{
    fixture::{seed_linear_workflow, test_pool},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator, TestAgentEvents},
};
use pontia_storage_sqlite::{
    models::workflows::WorkflowRecoveryRow, repositories::workflows::SqliteWorkflowRepository,
};
use pontia_workflow::{WorkflowCoordinator, WorkflowQueryService, WorkflowRecoveryService};

async fn fixture() -> (
    tempfile::TempDir,
    pontia_application::AppState,
    SqliteWorkflowRepository,
) {
    let root = tempfile::tempdir().unwrap();
    let pool = test_pool(&root.path().join("recovery.db")).await;
    let repo = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repo, "wf_retry", "[]", true).await;
    repo.start_workflow("wf_retry", "started").await.unwrap();
    for (id, state) in [("upstream", "exited"), ("failed", "exited")] {
        sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES (?,'pi',?)")
            .bind(id)
            .bind(state)
            .execute(&pool)
            .await
            .unwrap();
    }
    repo.bind_node_session("wf_retry_root", "upstream")
        .await
        .unwrap();
    repo.record_node_submission("wf_retry_root", "upstream-runtime", "upstream-submitted")
        .await
        .unwrap();
    repo.bind_node_session("wf_retry_child", "failed")
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key) VALUES ('binding','failed','pi','/workspace','native-session')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings(session_id,runtime_kind,runtime_instance_id,binding_state) VALUES ('failed','pi_tui','old-runtime','confirmed')").execute(&pool).await.unwrap();
    exit(&pool, "old-exit", "old-runtime").await;
    repo.fail_unsubmitted_workflow_node(
        "wf_retry",
        "wf_retry_child",
        "failure",
        "Agent Client reported session.exited before Agent Node wf_retry_child Submission",
        "old-exit",
        Some("old-runtime"),
    )
    .await
    .unwrap();
    let handoff = root.path().join("workflows/wf_retry/handoff");
    std::fs::create_dir_all(&handoff).unwrap();
    std::fs::write(handoff.join("root.md"), "valid upstream output").unwrap();
    std::fs::write(handoff.join("child.md"), "partial output").unwrap();
    let app = pontia_application::AppState::builder(pool, root.path().into())
        .clients(super::test_doubles::clients())
        .build();
    (root, app, repo)
}

async fn exit(pool: &sqlx::SqlitePool, id: &str, runtime: &str) {
    sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at,payload) VALUES (?,'failed','runtime_manager','pi','session.exited','2026-09-24T00:00:00Z',?)")
        .bind(id).bind(serde_json::json!({"runtime_instance_id":runtime}).to_string()).execute(pool).await.unwrap();
}

async fn ready(
    app: &pontia_application::AppState,
    repo: &SqliteWorkflowRepository,
    row: &WorkflowRecoveryRow,
) {
    assert!(
        repo.claim_recovery_preparation(&row.recovery_id)
            .await
            .unwrap()
    );
    sqlx::query("UPDATE sessions SET state='idle' WHERE session_id='failed'")
        .execute(&app.db())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE runtime_bindings SET runtime_instance_id='new-runtime' WHERE session_id='failed'",
    )
    .execute(&app.db())
    .await
    .unwrap();
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES (?,'failed','resuming','after_idle','recover')").bind(&row.message_id).execute(&app.db()).await.unwrap();
    repo.start_recovery_delivery(&row.recovery_id)
        .await
        .unwrap();
}

fn coordinator(
    app: &pontia_application::AppState,
) -> WorkflowCoordinator<
    SequencedSessionCreator,
    RecordingExitRequester,
    RecordingExitRequester,
    TestAgentEvents,
> {
    WorkflowCoordinator::with_services(
        app,
        SequencedSessionCreator::new([]),
        RecordingExitRequester::default(),
        TestAgentEvents::new(app.db()),
        app.pontia_home().into(),
    )
}

#[tokio::test]
async fn concurrent_retry_preserves_one_attempt_and_completed_work() {
    let (root, app, repo) = fixture().await;
    let service = WorkflowRecoveryService::new(&app);
    let (a, b) = tokio::join!(
        service.retry("wf_retry", "failure"),
        service.retry("wf_retry", "failure")
    );
    let a = a.unwrap();
    assert_eq!(a.recovery_id, b.unwrap().recovery_id);
    assert_eq!(repo.list_recoveries("wf_retry").await.unwrap().len(), 1);
    assert_eq!(
        repo.get_workflow("wf_retry").await.unwrap().unwrap().state,
        "recovering"
    );
    assert!(
        repo.get_node("wf_retry_root")
            .await
            .unwrap()
            .unwrap()
            .submitted_at
            .is_some()
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("workflows/wf_retry/handoff/root.md")).unwrap(),
        "valid upstream output"
    );
    assert!(repo.resume_workflow("wf_retry", "resume").await.is_err());
}

#[tokio::test]
async fn preparation_failure_archives_only_unsubmitted_output_and_does_not_loop() {
    let (root, app, repo) = fixture().await;
    let row = WorkflowRecoveryService::new(&app)
        .retry("wf_retry", "failure")
        .await
        .unwrap();
    let coordinator = coordinator(&app);
    coordinator.reconcile("wf_retry").await.unwrap();
    coordinator.reconcile("wf_retry").await.unwrap();
    let attempts = repo.list_recoveries("wf_retry").await.unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0].state, "failed");
    assert!(attempts[0].failure_message.is_some());
    assert_eq!(
        std::fs::read_to_string(root.path().join(format!(
            "workflows/wf_retry/recoveries/{}/child.md",
            row.recovery_id
        )))
        .unwrap(),
        "partial output"
    );
    assert!(
        !root
            .path()
            .join("workflows/wf_retry/handoff/child.md")
            .exists()
    );
    assert!(
        root.path()
            .join("workflows/wf_retry/handoff/root.md")
            .exists()
    );
    assert!(
        repo.get_node("wf_retry_child")
            .await
            .unwrap()
            .unwrap()
            .submitted_at
            .is_none()
    );
}

#[tokio::test]
async fn recovered_execution_ignores_old_exit_and_rejects_old_submission_then_completes() {
    let (_root, app, repo) = fixture().await;
    let row = repo
        .request_recovery("wf_retry", "failure", "attempt")
        .await
        .unwrap();
    ready(&app, &repo, &row).await;
    assert!(
        repo.pause_workflow("wf_retry", "pause-too-early")
            .await
            .is_err()
    );
    repo.finish_recovery(&row.recovery_id, None).await.unwrap();
    let coordinator = coordinator(&app);
    coordinator.reconcile("wf_retry").await.unwrap();
    assert_eq!(
        repo.get_workflow("wf_retry").await.unwrap().unwrap().state,
        "running"
    );
    assert!(
        repo.fail_unsubmitted_workflow_node(
            "wf_retry",
            "wf_retry_child",
            "stale-failure",
            "old exit",
            "old-exit",
            Some("old-runtime")
        )
        .await
        .is_err()
    );
    assert!(
        repo.record_node_submission("wf_retry_child", "old-runtime", "stale-submit")
            .await
            .is_err()
    );
    repo.record_node_submission("wf_retry_child", "new-runtime", "submit")
        .await
        .unwrap();
    assert!(
        repo.record_node_submission("wf_retry_child", "new-runtime", "duplicate-submit")
            .await
            .is_err()
    );
    exit(&app.db(), "new-exit", "new-runtime").await;
    coordinator.reconcile("wf_retry").await.unwrap();
    assert_eq!(
        repo.get_workflow("wf_retry").await.unwrap().unwrap().state,
        "completed"
    );
    let timeline = WorkflowQueryService::new(app.db())
        .get_workflow_timeline("wf_retry")
        .await
        .unwrap()
        .unwrap();
    assert!(timeline.entries.iter().any(|e| e.event_id == "failure"));
    assert!(
        timeline
            .entries
            .iter()
            .any(|e| e.payload["failure_event_id"] == "failure")
    );
}

#[tokio::test]
async fn second_exit_is_a_new_failure_and_old_retry_cannot_start_another_attempt() {
    let (_root, app, repo) = fixture().await;
    let row = repo
        .request_recovery("wf_retry", "failure", "first")
        .await
        .unwrap();
    ready(&app, &repo, &row).await;
    repo.finish_recovery(&row.recovery_id, None).await.unwrap();
    sqlx::query("UPDATE inbox_messages SET state='failed' WHERE message_id=?")
        .bind(&row.message_id)
        .execute(&app.db())
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='failed'")
        .execute(&app.db())
        .await
        .unwrap();
    exit(&app.db(), "second-exit", "new-runtime").await;
    coordinator(&app).reconcile("wf_retry").await.unwrap();
    let candidate = repo.recovery_candidate("wf_retry").await.unwrap().unwrap();
    assert_ne!(candidate.failure_event_id, "failure");
    let old = repo
        .request_recovery("wf_retry", "failure", "ignored")
        .await
        .unwrap();
    assert_eq!(old.recovery_id, row.recovery_id);
    assert_eq!(
        repo.get_workflow("wf_retry").await.unwrap().unwrap().state,
        "failed"
    );
    let next = repo
        .request_recovery("wf_retry", &candidate.failure_event_id, "second")
        .await
        .unwrap();
    assert_eq!(next.exit_event_id, "second-exit");
}

#[tokio::test]
async fn restart_during_preparation_fails_diagnostically_without_replaying() {
    let (_root, app, repo) = fixture().await;
    let row = repo
        .request_recovery("wf_retry", "failure", "attempt")
        .await
        .unwrap();
    assert!(
        repo.claim_recovery_preparation(&row.recovery_id)
            .await
            .unwrap()
    );
    repo.recover_interrupted_preparations().await.unwrap();
    repo.recover_interrupted_preparations().await.unwrap();
    coordinator(&app).reconcile("wf_retry").await.unwrap();
    assert_eq!(
        repo.list_recoveries("wf_retry").await.unwrap()[0].state,
        "failed"
    );
    assert!(
        app.inbox_commands()
            .get_message("failed", &row.message_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        repo.get_workflow("wf_retry")
            .await
            .unwrap()
            .unwrap()
            .failure_message
            .unwrap()
            .contains("restart")
    );
}

#[tokio::test]
async fn unknown_input_and_unrelated_failure_are_not_retryable() {
    let (_root, app, repo) = fixture().await;
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,state,delivery_policy,input_summary) VALUES ('uncertain','failed','unknown','after_idle','input')").execute(&app.db()).await.unwrap();
    assert!(repo.recovery_candidate("wf_retry").await.unwrap().is_none());
    assert!(
        repo.request_recovery("wf_retry", "failure", "attempt")
            .await
            .is_err()
    );
    sqlx::query("UPDATE inbox_messages SET state='failed' WHERE message_id='uncertain'")
        .execute(&app.db())
        .await
        .unwrap();
    sqlx::query("UPDATE workflow_events SET payload='{\"failure_message\":\"graceful exit request failed\"}' WHERE event_id='failure'").execute(&app.db()).await.unwrap();
    assert!(repo.recovery_candidate("wf_retry").await.unwrap().is_none());
}

#[tokio::test]
async fn restart_between_ready_and_delivery_preserves_failed_or_unknown_input() {
    for prior_state in ["resuming", "dispatching"] {
        let (_root, app, repo) = fixture().await;
        let row = repo
            .request_recovery("wf_retry", "failure", "attempt")
            .await
            .unwrap();
        ready(&app, &repo, &row).await;
        sqlx::query("UPDATE inbox_messages SET state=? WHERE message_id=?")
            .bind(prior_state)
            .bind(&row.message_id)
            .execute(&app.db())
            .await
            .unwrap();
        app.inbox_commands().recover_deliveries().await.unwrap();
        coordinator(&app).reconcile("wf_retry").await.unwrap();
        coordinator(&app).reconcile("wf_retry").await.unwrap();
        assert_eq!(
            repo.get_workflow("wf_retry").await.unwrap().unwrap().state,
            "failed"
        );
        let input = app
            .inbox_commands()
            .get_message("failed", &row.message_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            input.state,
            if prior_state == "resuming" {
                "failed"
            } else {
                "unknown"
            }
        );
        assert_eq!(repo.list_recoveries("wf_retry").await.unwrap().len(), 1);
        assert_eq!(
            repo.recovery_candidate("wf_retry").await.unwrap().is_some(),
            prior_state == "resuming"
        );
    }
}
