use pontia_application::{
    AppState, CreateSessionRequest, client_contract::test_registration, clients::ClientRegistry,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn omitted_client_selection_uses_the_registered_default_without_global_state() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(test_registration());
    let state = AppState::builder(pool, root.path().to_path_buf())
        .clients(clients)
        .build();
    let request: CreateSessionRequest = serde_json::from_value(json!({})).unwrap();
    let result = state
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    assert_eq!(result.data["session"]["client_type"], "generic");
    assert_eq!(result.data["session"]["state"], "idle");
}

#[test]
fn explicit_empty_or_null_client_selection_is_not_reinterpreted_as_a_default() {
    for selection in [json!(""), json!(null)] {
        assert!(
            serde_json::from_value::<CreateSessionRequest>(json!({ "client_type": selection }))
                .is_err()
        );
    }
}
