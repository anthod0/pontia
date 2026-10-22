use pontia::client_maintenance::maintain_enabled;
use pontia_application::agent_clients::AgentClientDetails;
use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{cell::RefCell, collections::HashMap, path::Path};

#[derive(Default)]
struct Platform {
    commands: RefCell<Vec<(String, Vec<String>)>>,
    fail: bool,
}
impl CommandRunner for Platform {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        self.commands
            .borrow_mut()
            .push((program.into(), args.to_vec()));
        Ok(CommandOutput {
            code: if self.fail { 1 } else { 0 },
            stdout: String::new(),
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
    serde_json::from_value(serde_json::json!({"id":"opaque","client_type":"pi","client_key":"default","name":"Pi","short_id":"pi","has_record":has_record,"enabled":enabled,"config_overrides":{},"effective_config":{},"created_at":null,"updated_at":null})).unwrap()
}

#[test]
fn only_explicitly_enabled_integrations_maintain_artifacts_not_client_binaries() {
    let root = tempfile::tempdir().unwrap();
    for (has_record, enabled) in [(false, true), (true, false), (true, true)] {
        let platform = Platform::default();
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
                    (
                        "pi".into(),
                        vec!["install".into(), "npm:@pontia/pi-client-plugin".into()]
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
