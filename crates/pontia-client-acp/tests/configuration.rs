use pontia_application::client_contract::ClientConfiguration;
use pontia_client_acp::AcpAgentConfig;
use pontia_client_acp::configuration::AcpConfiguration;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn partial_overrides_inherit_and_arrays_replace_without_affecting_other_peers() {
    let configuration = AcpConfiguration::new(BTreeMap::from([(
        "kimi".into(),
        json!({"env":{"A":"file", "B":"keep"},"name":"File name"}),
    )]));
    let value = configuration
        .resolve("kimi", &json!({"args":[],"env":{"A":"db"}}))
        .unwrap();
    assert_eq!(value["command"], "kimi");
    assert_eq!(value["name"], "File name");
    assert_eq!(value["args"], json!([]));
    assert_eq!(value["env"], json!({"A":"db","B":"keep"}));
    assert_eq!(
        configuration.resolve("kimi", &json!({"env":{}})).unwrap()["env"],
        json!({"A":"file","B":"keep"})
    );
    assert_eq!(
        configuration.resolve("opencode", &json!({})).unwrap()["args"],
        json!(["acp"])
    );
    assert_eq!(
        configuration.resolve("kimi", &json!({})).unwrap()["args"],
        json!(["acp"])
    );
}

#[test]
fn explicit_clear_switches_launch_source_and_removes_inherited_environment() {
    let configuration = AcpConfiguration::new(BTreeMap::from([(
        "kimi".into(),
        json!({"env":{"A":"file", "B":"keep"}}),
    )]));
    assert!(
        configuration
            .resolve("kimi", &json!({"registry_id":"registry-kimi"}))
            .is_err()
    );
    let value = configuration
        .resolve(
            "kimi",
            &json!({"command":null,"args":null,"registry_id":"registry-kimi","env":{"A":null}}),
        )
        .unwrap();
    let effective: AcpAgentConfig = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(value["command"], json!(null));
    assert_eq!(effective.registry_id.as_deref(), Some("registry-kimi"));
    assert!(effective.command.is_empty());
    assert_eq!(effective.env, BTreeMap::from([("B".into(), "keep".into())]));
    let value = configuration.resolve("kimi", &json!({"env":null})).unwrap();
    assert!(
        serde_json::from_value::<AcpAgentConfig>(value)
            .unwrap()
            .env
            .is_empty()
    );
}

#[test]
fn database_only_peer_requires_one_valid_source_after_merging() {
    let configuration = AcpConfiguration::new(Default::default());
    let value = configuration
        .resolve("company", &json!({"command":"agent","args":["--acp"]}))
        .unwrap();
    assert_eq!(value["command"], "agent");
    for overrides in [
        json!({}),
        json!({"registry_id":""}),
        json!({"registry_id":"gemini","args":[]}),
        json!({"registry_id":"gemini","command":"agent"}),
        json!({"command":"agent","transport":"http"}),
        json!({"command":"agent","capabilities":{}}),
        json!({"command":"agent","cwd":"/fixed"}),
        json!({"command":""}),
        json!({"args":["--acp"]}),
        json!({"command":null}),
    ] {
        assert!(
            configuration.resolve("company", &overrides).is_err(),
            "accepted {overrides}"
        );
    }
    assert!(
        configuration
            .resolve("", &json!({"command":"agent"}))
            .is_err()
    );
    assert_eq!(
        configuration
            .key(&json!({"acp_agent_id":"company"}))
            .unwrap(),
        "company"
    );
}
