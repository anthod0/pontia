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
    use pontia_core::execution_policy::ExecutionPolicy;
    let agents = AcpAgents::builtin();
    let mut other = agents.get("grok").unwrap().clone();
    other.restricted_execution = None;
    let configuration = AcpConfiguration::new(AcpAgents::new([
        ("restricted".into(), agents.get("grok").unwrap().clone()),
        ("plain".into(), other),
    ]));
    let mut restricted = configuration
        .restricted_execution_policy("restricted")
        .unwrap();
    assert_eq!(restricted.disabled_tools.len(), 18);
    restricted.disable_skills = true;
    restricted.yolo = true;
    configuration
        .validate_execution_policy("restricted", &restricted)
        .unwrap();
    assert!(configuration.restricted_execution_policy("plain").is_none());
    assert!(
        configuration
            .validate_execution_policy("plain", &restricted)
            .is_err()
    );
    for policy in [
        ExecutionPolicy {
            disable_mcp: true,
            ..Default::default()
        },
        ExecutionPolicy {
            tools: Some(vec![]),
            ..Default::default()
        },
        ExecutionPolicy {
            disabled_tools: vec!["kill_task".into()],
            ..Default::default()
        },
        ExecutionPolicy {
            disabled_tools: vec!["unknown".into()],
            ..Default::default()
        },
    ] {
        assert!(
            configuration
                .validate_execution_policy("restricted", &policy)
                .is_err()
        );
    }
}

#[test]
fn antigravity_accepts_its_fixed_policy_in_managed_directories() {
    let configuration = AcpConfiguration::new(AcpAgents::builtin());
    let mut policy = configuration
        .restricted_execution_policy("antigravity")
        .unwrap();
    policy.disable_skills = true;
    policy.yolo = true;
    policy.disabled_tools.reverse();
    configuration
        .validate_execution_policy("antigravity", &policy)
        .unwrap();
    policy.disabled_tools.pop();
    assert!(
        configuration
            .validate_execution_policy("antigravity", &policy)
            .is_err()
    );
}
