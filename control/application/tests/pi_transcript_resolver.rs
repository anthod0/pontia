use std::fs;

use pontia_agent_clients::{
    pi::raw_transcripts::PiAgentBindingResolver,
    raw_transcripts::{AgentBindingResolveRequest, AgentBindingResolver},
};
use tempfile::tempdir;

#[test]
fn pi_resolver_uses_client_session_file_directly() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session-with-unrelated-name.jsonl");
    fs::write(&session_file, "{\"type\":\"session\",\"version\":3}\n").unwrap();

    let source = PiAgentBindingResolver::new()
        .resolve(&AgentBindingResolveRequest {
            id: "bind_1".to_string(),
            session_id: "sess_1".to_string(),
            client_type: "pi".to_string(),
            client_session_file: Some(session_file.clone()),
        })
        .unwrap();

    assert_eq!(source.id, "bind_1");
    assert_eq!(source.client_type, "pi");
    assert_eq!(source.format, "pi-jsonl");
    assert_eq!(source.path, session_file);
}

#[test]
fn pi_resolver_rejects_missing_client_session_file() {
    let error = PiAgentBindingResolver::new()
        .resolve(&AgentBindingResolveRequest {
            id: "bind_1".to_string(),
            session_id: "sess_1".to_string(),
            client_type: "pi".to_string(),
            client_session_file: None,
        })
        .unwrap_err();

    assert!(error.to_string().contains("source_unavailable:"));
    assert!(error.to_string().contains("client_session_file"));
}
