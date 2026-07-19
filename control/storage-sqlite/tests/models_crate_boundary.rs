use pontia_storage_sqlite::{
    connect_sqlite,
    models::{events::EventRow, sessions::SessionRow},
};

#[test]
fn storage_sqlite_crate_exposes_row_models() {
    let session = SessionRow {
        session_id: "sess_test".to_string(),
        client_type: "pi".to_string(),
        title: None,
        handle: None,
        role: None,
        description: None,
        execution_profile_id: None,
        execution_profile_version: None,
        state: "created".to_string(),
        current_turn_id: None,
        workspace_id: None,
        workspace_ref: None,
        pinned_at: None,
        archived_at: None,
        metadata: "{}".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let event = EventRow {
        event_id: "evt_test".to_string(),
        session_id: session.session_id.clone(),
        turn_id: None,
        source: "external_api".to_string(),
        event_type: "session.created".to_string(),
        occurred_at: session.created_at.clone(),
        payload: "{}".to_string(),
    };
    assert_eq!(event.session_id, "sess_test");
}

#[test]
fn storage_sqlite_crate_exposes_connection_and_repositories() {
    let _connect = connect_sqlite;
}
