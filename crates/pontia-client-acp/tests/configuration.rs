use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_client_acp::{AcpAgents, configuration::AcpConfiguration};
use serde_json::json;

#[test]
fn grok_launch_and_management_settings() {
    let configuration = AcpConfiguration::new(AcpAgents::builtin());
    assert_eq!(configuration.keys(), ["grok"]);
    assert_eq!(
        configuration.resolve("grok", &json!({})).unwrap(),
        json!({"name":"Grok Build","command":"grok","args":["agent","stdio"],"env":{}})
    );
    assert!(
        configuration
            .resolve("grok", &json!({"command":"other"}))
            .is_err()
    );
    assert!(configuration.resolve("unknown", &json!({})).is_err());
    for key in [".", "..", ""] {
        assert!(configuration.key(&json!({"acp_agent_id":key})).is_err());
        assert!(configuration.resolve(key, &json!({})).is_err());
    }
}

#[test]
fn acp_omits_restricted_execution_policy_and_rejects_yolo() {
    let configuration = AcpConfiguration::new(AcpAgents::builtin());
    assert_eq!(configuration.restricted_execution_policy("grok"), None);
    assert_eq!(configuration.restricted_execution_policy("unknown"), None);
    configuration
        .validate_execution_policy(&Default::default())
        .unwrap();
    let yolo = pontia_agent_control::ExecutionPolicy {
        yolo: true,
        ..Default::default()
    };
    assert!(matches!(
        configuration.validate_execution_policy(&yolo),
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
}
