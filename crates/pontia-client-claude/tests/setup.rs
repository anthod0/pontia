use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{collections::HashMap, path::Path};

struct Runner {
    dependency_code: i32,
}
impl CommandRunner for Runner {
    fn run(&self, _: &str, args: &[String]) -> Result<CommandOutput, String> {
        Ok(CommandOutput {
            code: if args[0] == "-p" {
                0
            } else {
                self.dependency_code
            },
            stdout: "/opt/node/bin/node\n".into(),
            stderr: "sensitive native diagnostics".into(),
        })
    }
}
struct NoDefinitions;
impl DefinitionStore for NoDefinitions {
    fn read(&self, _: &Path) -> Result<Option<String>, String> {
        panic!("no independent service")
    }
    fn install(&self, _: &Path, _: &str) -> Result<bool, String> {
        panic!("no independent service")
    }
}

#[test]
fn setup_preserves_native_context_and_runtime_without_reading_credentials_or_creating_services() {
    let root = tempfile::tempdir().unwrap();
    let host = root.path().join("host.js");
    std::fs::write(&host, "fixture").unwrap();
    let config = root.path().join("native-config");
    let vars = HashMap::from([
        (
            "PONTIA_CLAUDE_HOST_PATH".into(),
            host.to_str().unwrap().into(),
        ),
        ("CLAUDE_CONFIG_DIR".into(), config.to_str().unwrap().into()),
    ]);
    let runner = Runner { dependency_code: 0 };
    let setup = pontia_client_claude::setup::integration()
        .prepare(&vars, root.path(), &runner)
        .unwrap();
    setup.preflight(&runner).unwrap();
    setup.install(&runner, &NoDefinitions).unwrap();
    let paths: HashMap<_, _> = setup.service_environment_paths().into_iter().collect();
    assert_eq!(paths["CLAUDE_CONFIG_DIR"], config);
    assert_eq!(
        paths["PONTIA_CLAUDE_NODE_PATH"],
        Path::new("/opt/node/bin/node")
    );
    assert!(!config.exists());
    let error = setup.preflight(&Runner { dependency_code: 1 }).unwrap_err();
    assert!(!error.contains("sensitive native diagnostics"));
    std::fs::remove_file(host).unwrap();
    assert!(setup.preflight(&runner).is_err());
}

#[test]
fn setup_defaults_to_the_same_users_native_configuration() {
    let root = tempfile::tempdir().unwrap();
    let runner = Runner { dependency_code: 0 };
    let setup = pontia_client_claude::setup::integration()
        .prepare(&HashMap::new(), root.path(), &runner)
        .unwrap();
    let paths: HashMap<_, _> = setup.service_environment_paths().into_iter().collect();
    assert_eq!(paths["CLAUDE_CONFIG_DIR"], root.path().join(".claude"));
    assert!(!paths.contains_key("PONTIA_CLAUDE_HOST_PATH"));
}
