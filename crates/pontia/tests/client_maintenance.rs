use pontia::client_maintenance::maintain_enabled;
use pontia_agent_control::agent_clients::AgentClientDetails;
use pontia_agent_control::client_contract::ClientDetection;
use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{cell::RefCell, collections::HashMap, path::Path};

#[derive(Default)]
struct Platform {
    commands: RefCell<Vec<(String, Vec<String>)>>,
    fail: bool,
    packages: String,
}
impl CommandRunner for Platform {
    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        _: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        self.run(program, args)
    }

    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        self.commands
            .borrow_mut()
            .push((program.into(), args.to_vec()));
        Ok(CommandOutput {
            code: if self.fail { 1 } else { 0 },
            stdout: if args == ["list"] {
                self.packages.clone()
            } else {
                String::new()
            },
            stderr: String::new(),
        })
    }
}
impl DefinitionStore for Platform {
    fn read(&self, _: &Path) -> Result<Option<String>, String> {
        panic!("unexpected service access")
    }
    fn install(&self, _: &Path, _: &str) -> Result<bool, String> {
        panic!("unexpected service modification")
    }
}
fn entry(has_record: bool, enabled: bool) -> AgentClientDetails {
    AgentClientDetails {
        id: "opaque".into(),
        client_type: "pi".into(),
        client_key: "default".into(),
        name: "Pi".into(),
        short_id: "pi".into(),
        has_record,
        enabled,
        detection: ClientDetection::detected("pi"),
        setup_command: None,
        config_overrides: serde_json::json!({}),
        effective_config: serde_json::json!({}),
        created_at: None,
        updated_at: None,
    }
}

#[test]
fn only_explicitly_enabled_integrations_maintain_artifacts_not_client_binaries() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"@pontia/pi-client-plugin","version":"0.1.0","pi":{"extensions":["index.ts"]}}"#,
    )
    .unwrap();
    std::fs::write(root.path().join("index.ts"), "export default () => {};").unwrap();
    for (has_record, enabled) in [(false, true), (true, false), (true, true)] {
        let platform = Platform {
            packages: format!(
                "User packages:\n  npm:@pontia/pi-client-plugin\n    {}\n",
                root.path().display()
            ),
            ..Default::default()
        };
        maintain_enabled(
            &[entry(has_record, enabled)],
            &pontia_clients::integrations(),
            &HashMap::new(),
            root.path(),
            &platform,
            &platform,
        )
        .unwrap();
        let calls = platform.commands.into_inner();
        if has_record && enabled {
            assert_eq!(
                calls,
                vec![
                    ("pi".into(), vec!["--version".into()]),
                    ("pi".into(), vec!["list".into()]),
                    (
                        "pi".into(),
                        vec![
                            "update".into(),
                            "npm:@pontia/pi-client-plugin".into(),
                            "--no-approve".into()
                        ]
                    )
                ]
            );
        } else {
            assert!(calls.is_empty());
        }
    }
}

#[test]
fn failed_maintenance_is_not_reported_as_success() {
    let root = tempfile::tempdir().unwrap();
    let platform = Platform {
        fail: true,
        ..Default::default()
    };
    assert!(
        maintain_enabled(
            &[entry(true, true)],
            &pontia_clients::integrations(),
            &HashMap::new(),
            root.path(),
            &platform,
            &platform
        )
        .is_err()
    );
}
