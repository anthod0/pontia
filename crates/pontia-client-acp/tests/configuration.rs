use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_client_acp::{AcpAgents, configuration::AcpConfiguration};
use serde_json::json;

#[test]
fn builtin_launch_and_management_settings() {
    let configuration = AcpConfiguration::new(AcpAgents::builtin());
    assert_eq!(configuration.keys(), ["antigravity", "grok", "kiro"]);
    assert_eq!(
        configuration.resolve("grok").unwrap(),
        json!({"name":"Grok Build","command":"grok","args":["agent","stdio"],"env":{}})
    );
    assert_eq!(
        configuration.resolve("antigravity").unwrap(),
        json!({"name":"Google Antigravity","command":"agy","args":["acp"],"env":{}})
    );
    assert_eq!(
        configuration.resolve("kiro").unwrap(),
        json!({"name":"Kiro","command":"kiro-cli","args":["acp","--agent-engine=v3"],"env":{}})
    );
    assert!(configuration.resolve("unknown").is_err());
    for key in [".", "..", ""] {
        assert!(configuration.key(&json!({"acp_agent_id":key})).is_err());
        assert!(configuration.resolve(key).is_err());
    }
}

#[test]
fn acp_validates_the_selected_definition() {
    let agents = AcpAgents::builtin();
    let mut other = agents.get("grok").unwrap().clone();
    other.restricted_execution = None;
    let configuration = AcpConfiguration::new(AcpAgents::new([
        ("restricted".into(), agents.get("grok").unwrap().clone()),
        ("plain".into(), other),
    ]));
    use pontia_core::SessionMode::{Default, Restricted, Yolo};
    for mode in [Default, Restricted, Yolo] {
        assert!(configuration.supports_mode("restricted", mode));
        assert_eq!(configuration.supports_mode("plain", mode), mode == Default);
    }
    let builtin = AcpConfiguration::new(AcpAgents::builtin());
    for key in ["kiro", "antigravity"] {
        assert!(builtin.supports_mode(key, Restricted));
        assert!(!builtin.supports_mode(key, Yolo));
    }
}
