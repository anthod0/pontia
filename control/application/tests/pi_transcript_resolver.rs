use std::fs;

use pontia_agent_clients::{
    pi::raw_transcripts::PiAgentBindingResolver,
    raw_transcripts::{AgentBindingResolveRequest, AgentBindingResolver},
};
use tempfile::tempdir;

fn pi_session_dir(agent_dir: &std::path::Path, cwd: &str) -> std::path::PathBuf {
    let safe = format!(
        "--{}--",
        cwd.trim_start_matches('/').replace(['/', '\\', ':'], "-")
    );
    agent_dir.join("sessions").join(safe)
}

#[test]
fn pi_resolver_finds_jsonl_for_launch_cwd_and_client_session_key() {
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let session_key = "11111111-2222-3333-4444-555555555555";
    let session_dir = pi_session_dir(&agent_dir, cwd.to_str().unwrap());
    fs::create_dir_all(&session_dir).unwrap();
    let session_file = session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&session_file, "{\"type\":\"session\",\"version\":3}\n").unwrap();

    let resolver = PiAgentBindingResolver::with_agent_dir(agent_dir.clone());
    let source = resolver
        .resolve(&AgentBindingResolveRequest {
            id: "bind_1".to_string(),
            session_id: "sess_1".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.clone(),
            client_session_key: session_key.to_string(),
        })
        .unwrap();

    assert_eq!(source.id, "bind_1");
    assert_eq!(source.client_type, "pi");
    assert_eq!(source.format, "pi-jsonl");
    assert_eq!(source.path, session_file);
}
