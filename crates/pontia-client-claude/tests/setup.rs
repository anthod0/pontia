use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

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

struct Installer {
    install_code: i32,
    dependency_code: i32,
    downloads: AtomicUsize,
}
impl CommandRunner for Installer {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        let code = if program == "npm" && args[0] == "install" {
            self.downloads.fetch_add(1, Ordering::SeqCst);
            let prefix = args.iter().position(|arg| arg == "--prefix").unwrap();
            let host = Path::new(&args[prefix + 1])
                .join("node_modules/@pontia/claude-client/dist/src/main.js");
            std::fs::create_dir_all(host.parent().unwrap()).unwrap();
            std::fs::write(host, "fixture").unwrap();
            self.install_code
        } else if args.last().is_some_and(|arg| arg == "preflight") {
            self.dependency_code
        } else {
            0
        };
        Ok(CommandOutput {
            code,
            stdout: "/opt/node/bin/node\n".into(),
            stderr: String::new(),
        })
    }
}

#[test]
fn installs_once_and_reuses_the_client_without_npm_downloads() {
    let root = tempfile::tempdir().unwrap();
    let runner = Installer {
        install_code: 0,
        dependency_code: 0,
        downloads: AtomicUsize::new(0),
    };
    let integration = pontia_client_claude::setup::integration();
    for _ in 0..2 {
        let setup = integration
            .prepare(&HashMap::new(), root.path(), &runner)
            .unwrap();
        setup.preflight(&runner).unwrap();
        setup.install(&runner, &NoDefinitions).unwrap();
    }
    assert_eq!(runner.downloads.load(Ordering::SeqCst), 1);
    assert!(
        root.path()
            .join(
                ".pontia/clients/claude-client/node_modules/@pontia/claude-client/dist/src/main.js"
            )
            .is_file()
    );
    assert!(!root.path().join(".claude").exists());
}

#[test]
fn failed_installation_or_dependency_check_leaves_no_partial_client_and_can_retry() {
    let root = tempfile::tempdir().unwrap();
    for (install_code, dependency_code) in [(1, 0), (0, 1), (0, 0)] {
        let runner = Installer {
            install_code,
            dependency_code,
            downloads: AtomicUsize::new(0),
        };
        let setup = pontia_client_claude::setup::integration()
            .prepare(&HashMap::new(), root.path(), &runner)
            .unwrap();
        setup.preflight(&runner).unwrap();
        assert!(!root.path().join(".pontia/clients/claude-client").exists());
        let result = setup.install(&runner, &NoDefinitions);
        if install_code != 0 || dependency_code != 0 {
            assert!(result.is_err());
            assert!(
                std::fs::read_dir(root.path().join(".pontia/clients"))
                    .unwrap()
                    .next()
                    .is_none()
            );
        } else {
            result.unwrap();
            setup.preflight(&runner).unwrap();
        }
    }
}
