use pontia_agent_control::{AgentControl, CreateSessionRequest};
use pontia_storage_sqlite::connect_sqlite;
use serde_json::json;

#[tokio::test]
async fn create_session_requires_a_client_type() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let state = AgentControl::builder(pool, root.path().into()).build();
    let mut request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"pi"})).unwrap();
    request.client_type.clear();
    assert!(matches!(
        state.session_commands().create_session(request).await,
        Err(pontia_core::Error::Domain(_))
    ));
}

#[test]
fn session_request_requires_a_nonempty_client_type() {
    for request in [
        json!({}),
        json!({"client_type":""}),
        json!({"client_type":null}),
    ] {
        assert!(serde_json::from_value::<CreateSessionRequest>(request).is_err());
    }
}
