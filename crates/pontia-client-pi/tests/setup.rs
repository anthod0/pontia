use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{cell::RefCell, collections::HashMap, path::Path};

struct Runner {
    code: i32,
    calls: RefCell<Vec<(String, Vec<String>)>>,
}
impl CommandRunner for Runner {
    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        _: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        self.run(program, args)
    }

    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        self.calls
            .borrow_mut()
            .push((program.into(), args.to_vec()));
        Ok(CommandOutput {
            code: self.code,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
struct NoDefinitions;
impl DefinitionStore for NoDefinitions {
    fn read(&self, _path: &Path) -> Result<Option<String>, String> {
        panic!("Pi integration does not read service definitions")
    }
    fn install(&self, _path: &Path, _contents: &str) -> Result<bool, String> {
        panic!("Pi integration does not install service definitions")
    }
}

#[test]
fn preparation_is_read_only_and_installation_uses_the_existing_plugin_package() {
    let root = tempfile::tempdir().unwrap();
    let runner = Runner {
        code: 0,
        calls: RefCell::new(Vec::new()),
    };
    let setup = pontia_client_pi::setup::integration()
        .prepare(&HashMap::new(), root.path(), &runner)
        .unwrap();
    assert!(runner.calls.borrow().is_empty());
    setup.preflight(&runner).unwrap();
    setup.install(&runner, &NoDefinitions).unwrap();
    assert_eq!(
        runner.calls.into_inner(),
        vec![
            ("pi".into(), vec!["--version".into()]),
            ("pi".into(), vec!["list".into()]),
            (
                "pi".into(),
                vec!["install".into(), "npm:@pontia/pi-client-plugin".into()]
            ),
        ]
    );
    assert!(setup.service_environment_paths().is_empty());
}

#[test]
fn preflight_and_installation_propagate_native_command_failure() {
    let root = tempfile::tempdir().unwrap();
    let runner = Runner {
        code: 1,
        calls: RefCell::new(Vec::new()),
    };
    let setup = pontia_client_pi::setup::integration()
        .prepare(&HashMap::new(), root.path(), &runner)
        .unwrap();
    assert!(setup.preflight(&runner).is_err());
    assert!(setup.install(&runner, &NoDefinitions).is_err());
}

#[cfg(unix)]
#[test]
fn version_preflight_ignores_output_encoding_but_respects_exit_status() {
    use pontia_runtime::local_service::ProcessCommandRunner;
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    struct VersionWrapper(PathBuf);
    impl CommandRunner for VersionWrapper {
        fn run(&self, _program: &str, args: &[String]) -> Result<CommandOutput, String> {
            ProcessCommandRunner.run(self.0.to_str().unwrap(), args)
        }
        fn run_status(&self, _program: &str, args: &[String]) -> Result<i32, String> {
            ProcessCommandRunner.run_status(self.0.to_str().unwrap(), args)
        }
    }

    let root = tempfile::tempdir().unwrap();
    for (code, succeeds) in [(0, true), (7, false)] {
        let path = root.path().join(format!("pi-wrapper-{code}"));
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf '\\377'\nprintf '\\376' >&2\nexit {code}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let runner = VersionWrapper(path);
        let setup = pontia_client_pi::setup::integration()
            .prepare(&HashMap::new(), root.path(), &runner)
            .unwrap();

        assert_eq!(setup.preflight(&runner).is_ok(), succeeds);
    }
}

struct Packages {
    directory: std::path::PathBuf,
    source: String,
    installs: RefCell<Vec<String>>,
}

impl CommandRunner for Packages {
    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        _: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        self.run(program, args)
    }

    fn run(&self, _: &str, args: &[String]) -> Result<CommandOutput, String> {
        let stdout = if args[0] == "list" {
            format!(
                "User packages:\n  {}\n    {}\n",
                self.source,
                self.directory.display()
            )
        } else {
            if args[0] == "install" {
                self.installs.borrow_mut().push(args[1].clone());
                std::fs::write(self.directory.join("index.ts"), "export default () => {};")
                    .unwrap();
            }
            String::new()
        };
        Ok(CommandOutput {
            code: 0,
            stdout,
            stderr: String::new(),
        })
    }
}

#[test]
fn reuses_installed_npm_and_local_plugins_and_repairs_missing_npm_entrypoint() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"name":"@pontia/pi-client-plugin","version":"1.2.3","pi":{"extensions":["index.ts"]}}"#,
    )
    .unwrap();
    std::fs::write(root.path().join("index.ts"), "export default () => {};").unwrap();
    for source in [
        "npm:@pontia/pi-client-plugin".into(),
        root.path().display().to_string(),
    ] {
        let runner = Packages {
            directory: root.path().into(),
            source,
            installs: RefCell::new(Vec::new()),
        };
        let setup = pontia_client_pi::setup::integration()
            .prepare(&HashMap::new(), root.path(), &runner)
            .unwrap();
        setup.install(&runner, &NoDefinitions).unwrap();
        setup.install(&runner, &NoDefinitions).unwrap();
        assert!(runner.installs.borrow().is_empty());
    }
    std::fs::remove_file(root.path().join("index.ts")).unwrap();
    let runner = Packages {
        directory: root.path().into(),
        source: "npm:@pontia/pi-client-plugin".into(),
        installs: RefCell::new(Vec::new()),
    };
    let setup = pontia_client_pi::setup::integration()
        .prepare(&HashMap::new(), root.path(), &runner)
        .unwrap();
    setup.install(&runner, &NoDefinitions).unwrap();
    setup.install(&runner, &NoDefinitions).unwrap();
    assert_eq!(
        *runner.installs.borrow(),
        ["npm:@pontia/pi-client-plugin@1.2.3"]
    );
    assert!(root.path().join("index.ts").is_file());
}
