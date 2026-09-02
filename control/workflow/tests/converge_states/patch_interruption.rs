use std::{
    fs,
    future::Future,
    sync::{Arc, Mutex},
};

use pontia_storage_sqlite::repositories::{
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
    workflows::SqliteWorkflowRepository,
};
use pontia_workflow::{
    RequestWorkflowPatch, TurnInterruptionRequester, WorkflowCoordinator, WorkflowPatchService,
};
use serde_json::json;

use crate::{
    fixture::{seed_linear_workflow, test_pool},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator, TestAgentEvents},
};

#[derive(Clone, Default)]
struct RecordingInterrupter {
    requests: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl TurnInterruptionRequester for RecordingInterrupter {
    fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        self.requests.lock().expect("interruptions lock").push((
            session_id.to_string(),
            turn_id.to_string(),
            runtime_instance_id.to_string(),
        ));
        async { Ok(()) }
    }
}

#[tokio::test]
async fn only_the_exact_client_confirmed_interruption_unlocks_replanning() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    fs::create_dir(&pontia_home).expect("Pontia home");
    let pool = test_pool(&temp.path().join("patch-interruption.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_patch_interrupt", "[]", false).await;
    repository
        .start_workflow("wf_patch_interrupt", "evt_started")
        .await
        .expect("start Workflow");
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_patch_interrupt', 'pi', 'working')",
    )
    .execute(&pool)
    .await
    .expect("create Session");
    repository
        .bind_node_session("wf_patch_interrupt_root", "sess_patch_interrupt")
        .await
        .expect("bind requester");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, topology_status)
           VALUES ('turn_patch_interrupt', 'sess_patch_interrupt', 'running', 'root')"#,
    )
    .execute(&pool)
    .await
    .expect("create active Turn");
    SqliteRuntimeBindingRepository::new(pool.clone())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_patch_interrupt".into(),
            runtime_kind: "pi_tui".into(),
            runtime_instance_id: Some("runtime_patch_interrupt".into()),
            binding_state: "confirmed".into(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: None,
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            capabilities: "{}".into(),
            diagnostics: "{}".into(),
            adapter_details: "{}".into(),
        })
        .await
        .expect("runtime binding");
    let workflow_dir = pontia_home.join("workflows/wf_patch_interrupt");
    fs::create_dir_all(&workflow_dir).expect("Workflow directory");
    fs::write(
        workflow_dir.join("workflow.toml"),
        "workflow_id = \"wf_patch_interrupt\"\n",
    )
    .expect("definition surface");

    let outcome = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_patch_interrupt".into(),
            runtime_instance_id: "runtime_patch_interrupt".into(),
            document: "Need a corrected plan".into(),
        })
        .await
        .expect("request Patch");
    let interruptions = RecordingInterrupter::default();
    let coordinator = WorkflowCoordinator::with_services_and_interruptions(
        pool.clone(),
        SequencedSessionCreator::new([]),
        RecordingExitRequester::default(),
        interruptions.clone(),
        TestAgentEvents::new(pool.clone()),
        pontia_home,
    );
    assert!(interruptions.requests.lock().expect("requests").is_empty());

    coordinator
        .reconcile("wf_patch_interrupt")
        .await
        .expect("request interruption");
    assert_eq!(
        interruptions.requests.lock().expect("requests").as_slice(),
        &[(
            "sess_patch_interrupt".into(),
            "turn_patch_interrupt".into(),
            "runtime_patch_interrupt".into(),
        )]
    );
    let patch = repository
        .get_patch(&outcome.patch_id)
        .await
        .expect("Patch query")
        .expect("Patch");
    assert!(patch.interruption_requested_at.is_some());
    assert!(patch.replanning_unlocked_at.is_none());

    insert_interrupted_fact(&pool, "evt_wrong_runtime", "stale_runtime").await;
    coordinator
        .reconcile("wf_patch_interrupt")
        .await
        .expect("ignore stale fact");
    assert!(
        repository
            .get_patch(&outcome.patch_id)
            .await
            .expect("Patch query")
            .expect("Patch")
            .replanning_unlocked_at
            .is_none()
    );

    insert_interrupted_fact(&pool, "evt_confirmed", "runtime_patch_interrupt").await;
    coordinator
        .reconcile("wf_patch_interrupt")
        .await
        .expect("confirm interruption");
    assert!(
        repository
            .get_patch(&outcome.patch_id)
            .await
            .expect("Patch query")
            .expect("Patch")
            .replanning_unlocked_at
            .is_some()
    );
    assert_eq!(
        repository
            .get_workflow("wf_patch_interrupt")
            .await
            .expect("Workflow query")
            .expect("Workflow")
            .state,
        "replanning"
    );
    assert!(
        repository
            .list_events("wf_patch_interrupt")
            .await
            .expect("events")
            .iter()
            .all(|event| event.event_type != "workflow.failed")
    );
}

async fn insert_interrupted_fact(pool: &sqlx::SqlitePool, event_id: &str, runtime: &str) {
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
           VALUES (?, 'sess_patch_interrupt', 'turn_patch_interrupt', 'agent_adapter', 'pi',
                   'turn.interrupted', '2026-08-01T00:00:00Z', ?)"#,
    )
    .bind(event_id)
    .bind(json!({ "runtime_instance_id": runtime }).to_string())
    .execute(pool)
    .await
    .expect("persist interruption fact");
}
