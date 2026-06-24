use pontia_application::{AppState, app};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn app_state_is_constructed_through_builder() {
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();

    let state = AppState::builder(db.clone()).build();

    let fetched: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(fetched, 1);
}

#[tokio::test]
async fn app_state_is_available_from_app_namespace() {
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();

    let state: app::AppState = AppState::builder(db.clone()).build();

    let fetched: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(fetched, 1);
}

#[test]
fn application_exposes_artifacts_namespace() {
    fn assert_discovery_service(_: pontia_application::artifacts::ArtifactDiscoveryService) {}
    fn assert_registration_service(_: pontia_application::artifacts::ArtifactRegistrationService) {}
    fn assert_content_service(_: pontia_application::artifacts::ArtifactContentService) {}

    let _ = assert_discovery_service;
    let _ = assert_registration_service;
    let _ = assert_content_service;
}

#[test]
fn application_exposes_workspaces_namespace() {
    fn assert_browser_service(_: pontia_application::workspaces::WorkspaceBrowserService) {}
    fn assert_register_request(_: pontia_application::workspaces::RegisterWorkspaceRequest) {}
    fn assert_workspace_record(_: pontia_application::workspaces::WorkspaceRecord) {}

    let _ = assert_browser_service;
    let _ = assert_register_request;
    let _ = assert_workspace_record;
}

#[test]
fn application_exposes_command_namespaces() {
    fn assert_session_service(_: pontia_application::sessions::SessionCommandService) {}
    fn assert_task_service(_: pontia_application::tasks::TaskCommandService) {}
    fn assert_runtime_control(_: pontia_application::runtime_control::RuntimeControlService) {}

    let _ = assert_session_service;
    let _ = assert_task_service;
    let _ = assert_runtime_control;
}

#[test]
fn application_exposes_query_namespace() {
    fn assert_query_service(_: pontia_application::queries::ExternalQueryService) {}

    let _ = assert_query_service;
}

#[test]
fn application_exposes_view_submodule_namespaces() {
    fn assert_session_view(_: pontia_application::views::sessions::SessionView) {}
    fn assert_workspace_view(_: pontia_application::views::workspaces::WorkspaceView) {}
    fn assert_task_view(_: pontia_application::views::tasks::TaskView) {}
    fn assert_turn_view(_: pontia_application::views::turns::TurnView) {}
    fn assert_inbox_view(_: pontia_application::views::inbox::InboxMessageView) {}
    fn assert_event_view(_: pontia_application::views::events::EventView) {}
    fn assert_artifact_view(_: pontia_application::views::artifacts::ArtifactView) {}

    let _ = assert_session_view;
    let _ = assert_workspace_view;
    let _ = assert_task_view;
    let _ = assert_turn_view;
    let _ = assert_inbox_view;
    let _ = assert_event_view;
    let _ = assert_artifact_view;
}

#[test]
fn application_exposes_runtime_namespace() {
    fn assert_binding_service(_: pontia_application::runtime::RuntimeBindingUpsertService) {}
    fn assert_observation_service(_: pontia_application::runtime::RuntimeObservationService) {}
    fn assert_readiness_service(_: pontia_application::runtime::RuntimeReadinessService) {}

    let _ = assert_binding_service;
    let _ = assert_observation_service;
    let _ = assert_readiness_service;
}

#[test]
fn application_exposes_runtime_bindings_namespace() {
    fn assert_binding_service(
        _: pontia_application::runtime::bindings::RuntimeBindingUpsertService,
    ) {
    }
    fn assert_binding_request(
        _: pontia_application::runtime::bindings::RuntimeBindingUpsertRequest,
    ) {
    }

    let _ = assert_binding_service;
    let _ = assert_binding_request;
}

#[test]
fn application_exposes_runtime_bindings_submodule_namespaces() {
    fn assert_binding_service(
        _: pontia_application::runtime::bindings::service::RuntimeBindingUpsertService,
    ) {
    }
    fn assert_binding_request(
        _: pontia_application::runtime::bindings::types::RuntimeBindingUpsertRequest,
    ) {
    }

    let _ = assert_binding_service;
    let _ = assert_binding_request;
}

#[test]
fn application_exposes_runtime_observation_and_readiness_namespaces() {
    fn assert_observation_service(
        _: pontia_application::runtime::observation::RuntimeObservationService,
    ) {
    }
    fn assert_readiness_service(
        _: pontia_application::runtime::readiness::RuntimeReadinessService,
    ) {
    }

    let _ = assert_observation_service;
    let _ = assert_readiness_service;
}

#[test]
fn application_exposes_turns_namespace() {
    fn assert_turn_service(_: pontia_application::turns::TurnCommandService) {}
    fn assert_claim_service(_: pontia_application::turns::CurrentTurnClaimService) {}
    fn assert_claim_request(_: pontia_application::turns::CurrentTurnClaimRequest) {}

    let _ = assert_turn_service;
    let _ = assert_claim_service;
    let _ = assert_claim_request;
}

#[test]
fn application_exposes_turns_submodule_namespaces() {
    fn assert_command_service(_: pontia_application::turns::commands::TurnCommandService) {}
    fn assert_claim_service(_: pontia_application::turns::claim::CurrentTurnClaimService) {}
    fn assert_claim_request(_: pontia_application::turns::claim::CurrentTurnClaimRequest) {}

    let _ = assert_command_service;
    let _ = assert_claim_service;
    let _ = assert_claim_request;
}

#[test]
fn application_exposes_ingestion_namespace() {
    fn assert_ingest_service(_: pontia_application::ingestion::EventIngestService) {}
    fn assert_validation_service(_: pontia_application::ingestion::InternalEventValidationService) {
    }
    fn assert_ingest_result(_: pontia_application::ingestion::EventIngestResult) {}

    let _ = assert_ingest_service;
    let _ = assert_validation_service;
    let _ = assert_ingest_result;
}

#[test]
fn application_exposes_ingestion_submodule_namespaces() {
    fn assert_ingest_service(_: pontia_application::ingestion::service::EventIngestService) {}
    fn assert_validation_service(
        _: pontia_application::ingestion::validation::InternalEventValidationService,
    ) {
    }
    fn assert_ingest_result(_: pontia_application::ingestion::types::EventIngestResult) {}

    let _ = assert_ingest_service;
    let _ = assert_validation_service;
    let _ = assert_ingest_result;
}
