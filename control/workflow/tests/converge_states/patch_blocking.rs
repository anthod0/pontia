use std::sync::{Arc, Mutex};

use pontia_application::CreateSessionRequest;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{
    BlockWorkflowPatch, RequestWorkflowPatch, SessionCreator, WorkflowCoordinator,
    WorkflowPatchService,
};
use serde_json::{Value, json};

use crate::{
    fixture::{seed_linear_workflow, test_pool},
    test_doubles::{RecordingExitRequester, TestAgentEvents},
};

#[derive(Clone)]
struct PersistingSessionCreator {
    pool: sqlx::SqlitePool,
    requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
}

impl PersistingSessionCreator {
    fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool,
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl SessionCreator for PersistingSessionCreator {
    async fn find_session_by_creation_token(
        &self,
        metadata_key: &str,
        token: &str,
    ) -> pontia_workflow::Result<Option<String>> {
        let path = format!("$.{metadata_key}");
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE json_extract(metadata, ?) = ?",
        )
        .bind(path)
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(pontia_core::Error::from)?)
    }

    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        seed_replanner_session(
            &self.pool,
            "sess_replanner",
            "turn_replanner",
            "runtime_replanner",
            &request.metadata,
        )
        .await;
        Ok("sess_replanner".into())
    }
}

#[tokio::test]
async fn confirmed_interruption_creates_one_real_replanner_and_explicit_block_is_fenced() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).expect("Pontia home");
    let pool = test_pool(&temp.path().join("patch-blocking.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_block").await;

    let patch_id = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
            document: "The remaining plan cannot be completed safely.".into(),
        })
        .await
        .expect("request Patch")
        .patch_id;
    insert_fact(
        &pool,
        "evt_requester_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;

    let creator = PersistingSessionCreator::new(pool.clone());
    let exits = RecordingExitRequester::default();
    let coordinator = WorkflowCoordinator::with_services(
        pool.clone(),
        creator.clone(),
        exits.clone(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    coordinator
        .reconcile("wf_block")
        .await
        .expect("create Re-planner");
    coordinator
        .reconcile("wf_block")
        .await
        .expect("idempotent reconcile");

    {
        let requests = creator.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.client_type, "pi");
        assert_eq!(request.role.as_deref(), Some("workflow_replanner"));
        assert_eq!(request.workspace.as_deref(), Some("/workspace/project"));
        assert_eq!(request.metadata["workflow_id"], "wf_block");
        assert_eq!(request.metadata["workflow_patch_id"], patch_id);
        assert!(request.metadata["workflow_replanner_creation_token"].is_string());
        assert_eq!(
            request.runtime_environment["PONTIA_WORKFLOW_ID"],
            "wf_block"
        );
        assert_eq!(
            request.runtime_environment["PONTIA_WORKFLOW_PATCH_ID"],
            patch_id
        );
        assert!(
            request.runtime_environment["PONTIA_WORKFLOW_FILE"]
                .ends_with("/workflows/wf_block/workflow.toml")
        );
        assert!(
            request.runtime_environment["PONTIA_WORKFLOW_PATCH_REQUEST_FILE"]
                .ends_with(&format!("/patches/{patch_id}/request.md"))
        );
        let task = &request.initial_task.as_ref().expect("initial task").input;
        assert!(task.contains("pontia workflow show"));
        assert!(task.contains("patch apply --decision"));
        assert!(task.contains("patch block --reason"));
    }

    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "planning");
    assert_eq!(
        patch.replanner_session_id.as_deref(),
        Some("sess_replanner")
    );
    assert_eq!(
        patch.replanner_runtime_instance_id.as_deref(),
        Some("runtime_replanner")
    );
    assert!(
        repository
            .list_nodes("wf_block")
            .await
            .unwrap()
            .iter()
            .all(|node| node.session_id.as_deref() != Some("sess_replanner"))
    );

    let workflow_file = pontia_home.join("workflows/wf_block/workflow.toml");
    let accepted = std::fs::read_to_string(&workflow_file).unwrap();
    std::fs::write(&workflow_file, "unaccepted draft").unwrap();
    let outcome = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .block_patch(BlockWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
            reason: "No executable continuation exists.".into(),
        })
        .await
        .expect("block Patch");
    assert_eq!(outcome.patch_id, patch_id);
    assert_eq!(std::fs::read_to_string(&workflow_file).unwrap(), accepted);
    let patch_dir = pontia_home.join(format!("workflows/wf_block/patches/{patch_id}"));
    assert_eq!(
        std::fs::read_to_string(patch_dir.join("reason.md")).unwrap(),
        "No executable continuation exists."
    );
    assert_eq!(
        std::fs::read_to_string(patch_dir.join("blocked-draft.toml")).unwrap(),
        "unaccepted draft"
    );

    let blocked = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(blocked.state, "blocked");
    assert_eq!(blocked.replanner_turn_id.as_deref(), Some("turn_replanner"));
    assert_eq!(
        repository
            .get_workflow("wf_block")
            .await
            .unwrap()
            .unwrap()
            .state,
        "blocked"
    );
    assert_eq!(
        repository
            .list_events("wf_block")
            .await
            .unwrap()
            .last()
            .unwrap()
            .event_type,
        "workflow.patch_blocked"
    );

    let stale = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .block_patch(BlockWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
            reason: "overwrite".into(),
        })
        .await;
    assert!(stale.is_err());
    assert_eq!(
        std::fs::read_to_string(patch_dir.join("reason.md")).unwrap(),
        "No executable continuation exists."
    );

    insert_fact(
        &pool,
        "evt_replanner_completed",
        "sess_replanner",
        "turn_replanner",
        "turn.completed",
        "runtime_replanner",
    )
    .await;
    coordinator
        .reconcile("wf_block")
        .await
        .expect("request graceful exit");
    assert_eq!(
        exits.requests.lock().expect("exit requests").as_slice(),
        &[("sess_replanner".into(), "runtime_replanner".into())]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE session_id = 'sess_replanner' AND event_type = 'session.exited'")
            .fetch_one(&pool).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn crash_gap_recovers_the_session_with_the_persisted_creation_token() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("patch-replanner-recovery.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_recover").await;
    let patch_id = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
            document: "Recover planning".into(),
        })
        .await
        .unwrap()
        .patch_id;
    insert_fact(
        &pool,
        "evt_recover_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    let token = repository
        .get_patch(&patch_id)
        .await
        .unwrap()
        .unwrap()
        .replanner_creation_token;
    seed_replanner_session(
        &pool,
        "sess_recovered_replanner",
        "turn_recovered_replanner",
        "runtime_recovered_replanner",
        &json!({ "workflow_replanner_creation_token": token }),
    )
    .await;

    let creator = PersistingSessionCreator::new(pool.clone());
    WorkflowCoordinator::with_services(
        pool.clone(),
        creator.clone(),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool),
        pontia_home,
    )
    .reconcile("wf_recover")
    .await
    .unwrap();

    assert!(creator.requests.lock().unwrap().is_empty());
    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "planning");
    assert_eq!(
        patch.replanner_session_id.as_deref(),
        Some("sess_recovered_replanner")
    );
}

async fn seed_requester(
    pool: &sqlx::SqlitePool,
    repository: &SqliteWorkflowRepository,
    pontia_home: &std::path::Path,
    workflow_id: &str,
) {
    seed_linear_workflow(repository, workflow_id, "[]", false).await;
    repository
        .start_workflow(workflow_id, "evt_started")
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, current_turn_id) VALUES ('sess_requester', 'pi', 'busy', 'turn_requester')")
        .execute(pool).await.unwrap();
    repository
        .bind_node_session(&format!("{workflow_id}_root"), "sess_requester")
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn_requester', 'sess_requester', 'running', 'root')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES ('sess_requester', 'pi_tui', 'runtime_requester', 'confirmed')")
        .execute(pool).await.unwrap();
    let workflow_dir = pontia_home.join("workflows").join(workflow_id);
    std::fs::create_dir_all(&workflow_dir).unwrap();
    std::fs::write(
        workflow_dir.join("workflow.toml"),
        format!("workflow_id = \"{workflow_id}\"\nrevision = 1\n"),
    )
    .unwrap();
}

async fn seed_replanner_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_id: &str,
    metadata: &Value,
) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata) VALUES (?, 'pi', 'busy', ?, ?)")
        .bind(session_id).bind(turn_id).bind(metadata.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES (?, ?, 'running', 'root')")
        .bind(turn_id).bind(session_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES (?, 'pi_tui', ?, 'confirmed')")
        .bind(session_id).bind(runtime_id).execute(pool).await.unwrap();
}

async fn insert_fact(
    pool: &sqlx::SqlitePool,
    event_id: &str,
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    runtime_id: &str,
) {
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
           VALUES (?, ?, ?, 'agent_adapter', 'pi', ?, '2026-08-01T00:00:00Z', ?)"#,
    )
    .bind(event_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(event_type)
    .bind(json!({ "runtime_instance_id": runtime_id }).to_string())
    .execute(pool)
    .await
    .unwrap();
}
