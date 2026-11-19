#![cfg(target_os = "linux")]

use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use pontia::{
    lifecycle::ServiceManager,
    manager::{CommandOutput, CommandRunner, SystemdManager},
    uninstall::PreparedUninstall,
};

struct Runner {
    home: PathBuf,
    fail_stop: bool,
    fail_pi: bool,
    calls: RefCell<Vec<String>>,
}

impl CommandRunner for Runner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        let command = format!("{program} {}", args.join(" "));
        self.calls.borrow_mut().push(command.clone());
        if (self.fail_stop && command == "systemctl --user disable --now pontia.service")
            || (self.fail_pi && program == "pi" && args[0] == "remove")
        {
            return Ok(CommandOutput {
                code: 1,
                stdout: String::new(),
                stderr: "test failure".into(),
            });
        }
        let mut stdout = String::new();
        if program == "pi" {
            let settings_path = self.home.join(".pi/agent/settings.json");
            let mut settings: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
            if args[0] == "list" {
                stdout.push_str("User packages:\n");
                for package in settings["packages"].as_array().unwrap() {
                    let source = package.as_str().unwrap();
                    stdout.push_str(&format!(
                        "  {source}\n    {}\n",
                        self.home
                            .join("packages")
                            .join(source.strip_prefix("npm:").unwrap_or(source))
                            .display()
                    ));
                }
                stdout.push_str("Project packages:\n  npm:@pontia/pi-client-plugin\n");
            } else if args[0] == "remove" {
                settings["packages"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|source| source != &args[1]);
                fs::write(settings_path, serde_json::to_string(&settings).unwrap()).unwrap();
            } else {
                panic!("unexpected pi operation: {command}");
            }
        } else {
            assert_eq!(program, "systemctl");
            assert!(
                matches!(args[1].as_str(), "disable" | "daemon-reload"),
                "{command}"
            );
        }
        Ok(CommandOutput {
            code: 0,
            stdout,
            stderr: String::new(),
        })
    }

    fn run_with_env(
        &self,
        program: &str,
        args: &[String],
        environment: &[(String, String)],
    ) -> Result<CommandOutput, String> {
        assert!(environment.contains(&(
            "PI_CODING_AGENT_DIR".into(),
            self.home.join(".pi/agent").display().to_string()
        )));
        self.run(program, args)
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    pontia: PathBuf,
    shell: PathBuf,
    runner: Runner,
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("user");
        let pontia = root.path().join("custom pontia");
        let shell = root.path().join("custom-zsh/.zshrc");
        write(
            &shell,
            "export EDITOR=vi\n# >>> Pontia Codex PATH >>>\nexport PATH=/pontia/bin:$PATH\n# <<< Pontia Codex PATH <<<\nalias gs='git status'\n",
        );
        write(
            &pontia.join("bin/codex"),
            "#!/bin/sh\n# Pontia Codex launcher\nexit 0\n",
        );
        write(&pontia.join("env.sh"), "export PATH=/pontia/bin:$PATH\n");
        write(
            &pontia.join("state/codex-shim.json"),
            &serde_json::json!({"paths": [shell], "fish": false}).to_string(),
        );
        write(
            &pontia.join("clients/claude-client/node_modules/@pontia/claude-client/package.json"),
            r#"{"name":"@pontia/claude-client"}"#,
        );
        write(&pontia.join("config.toml"), "saved configuration");
        write(&pontia.join("auth.json"), "saved credential");
        write(&pontia.join("state/control-plane.db"), "saved history");
        write(&home.join(".codex/history.jsonl"), "native conversation");
        write(&home.join(".local/bin/pontia"), "installed CLI");
        write(&home.join(".local/bin/pontiad"), "installed daemon");
        write(
            &home.join(".pi/agent/settings.json"),
            r#"{"theme":"dark","packages":["npm:@pontia/pi-client-plugin@1.2.3","npm:other-plugin"]}"#,
        );
        let runner = Runner {
            home: home.clone(),
            fail_stop: false,
            fail_pi: false,
            calls: RefCell::new(Vec::new()),
        };
        let manager = SystemdManager::new(&runner);
        write(
            &manager.definition_path(&home),
            &manager
                .render_definition(
                    &home.join(".local/bin/pontiad"),
                    &pontia,
                    "https://auth.example.test",
                    None,
                )
                .unwrap(),
        );
        #[cfg(target_os = "linux")]
        write(
            &home.join(".config/systemd/user/pontia-codex.service"),
            "Codex autostart",
        );
        Self {
            _root: root,
            home,
            pontia,
            shell,
            runner,
        }
    }

    fn prepare(&self) -> PreparedUninstall {
        self.prepare_result().unwrap()
    }

    fn prepare_result(&self) -> Result<PreparedUninstall, String> {
        PreparedUninstall::prepare(
            &self.home,
            &HashMap::from([(
                "PONTIA_HOME".into(),
                self.home.join("unrelated").display().to_string(),
            )]),
            &SystemdManager::new(&self.runner),
            &self.runner,
            &pontia_clients::integrations(),
        )
    }
}

#[cfg(target_os = "linux")]
#[test]
fn cleanup_removes_integrations_and_preserves_data_binaries_and_other_shell_content() {
    let fixture = Fixture::new();
    let prepared = fixture.prepare();
    assert!(fixture.pontia.join("bin/codex").exists());
    assert!(
        fixture
            .runner
            .calls
            .borrow()
            .iter()
            .all(|call| call == "pi list")
    );
    prepared
        .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&fixture.shell).unwrap(),
        "export EDITOR=vi\nalias gs='git status'\n"
    );
    for path in [
        "bin/codex",
        "env.sh",
        "state/codex-shim.json",
        "clients/claude-client",
    ] {
        assert!(!fixture.pontia.join(path).exists(), "{path}");
    }
    for (path, contents) in [
        ("config.toml", "saved configuration"),
        ("auth.json", "saved credential"),
        ("state/control-plane.db", "saved history"),
    ] {
        assert_eq!(
            fs::read_to_string(fixture.pontia.join(path)).unwrap(),
            contents
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.home.join(".codex/history.jsonl")).unwrap(),
        "native conversation"
    );
    assert!(fixture.home.join(".local/bin/pontia").exists());
    assert!(fixture.home.join(".local/bin/pontiad").exists());
    assert!(
        !fixture
            .home
            .join(".config/systemd/user/pontia.service")
            .exists()
    );
    assert!(
        !fixture
            .home
            .join(".config/systemd/user/pontia-codex.service")
            .exists()
    );
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture.home.join(".pi/agent/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        settings,
        serde_json::json!({"theme":"dark","packages":["npm:other-plugin"]})
    );
    let retry = PreparedUninstall::prepare(
        &fixture.home,
        &HashMap::from([("PONTIA_HOME".into(), fixture.pontia.display().to_string())]),
        &SystemdManager::new(&fixture.runner),
        &fixture.runner,
        &pontia_clients::integrations(),
    )
    .unwrap();
    assert!(retry.summary().is_empty());
    retry
        .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
        .unwrap();
}

#[test]
fn stop_failure_preserves_all_integrations_and_service_definition() {
    let mut fixture = Fixture::new();
    fixture.runner.fail_stop = true;
    assert!(
        fixture
            .prepare()
            .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
            .is_err()
    );
    assert!(fixture.pontia.join("bin/codex").exists());
    assert!(fixture.pontia.join("clients/claude-client").exists());
    assert!(
        fixture
            .home
            .join(".config/systemd/user/pontia.service")
            .exists()
    );
    assert!(
        fs::read_to_string(&fixture.shell)
            .unwrap()
            .contains("# >>> Pontia Codex PATH >>>")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn client_failure_keeps_installed_home_available_for_retry() {
    let mut fixture = Fixture::new();
    fixture.runner.fail_pi = true;
    assert!(
        fixture
            .prepare()
            .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
            .is_err()
    );
    assert!(
        fixture
            .home
            .join(".config/systemd/user/pontia.service")
            .exists()
    );
    fixture.runner.fail_pi = false;
    fixture
        .prepare()
        .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
        .unwrap();
    assert!(!fixture.pontia.join("bin/codex").exists());
    assert!(
        !fixture
            .home
            .join(".config/systemd/user/pontia.service")
            .exists()
    );
}

#[test]
fn unmanaged_launcher_or_malformed_shell_block_refuses_cleanup_before_mutation() {
    let fixture = Fixture::new();
    write(
        &fixture.pontia.join("bin/codex"),
        "#!/bin/sh\n# my own launcher\n",
    );
    assert!(fixture.prepare_result().is_err());
    assert_eq!(
        fs::read_to_string(fixture.pontia.join("bin/codex")).unwrap(),
        "#!/bin/sh\n# my own launcher\n"
    );
    write(
        &fixture.pontia.join("bin/codex"),
        "#!/bin/sh\n# Pontia Codex launcher\n",
    );
    write(
        &fixture.shell,
        "export EDITOR=vi\n# >>> Pontia Codex PATH >>>\n",
    );
    assert!(fixture.prepare_result().is_err());
    assert_eq!(
        fs::read_to_string(&fixture.shell).unwrap(),
        "export EDITOR=vi\n# >>> Pontia Codex PATH >>>\n"
    );
    assert!(
        fixture
            .runner
            .calls
            .borrow()
            .iter()
            .all(|call| call == "pi list")
    );
}

#[test]
fn empty_installation_needs_no_client_executables_or_service_manager() {
    let root = tempfile::tempdir().unwrap();
    let runner = Runner {
        home: root.path().into(),
        fail_stop: true,
        fail_pi: true,
        calls: RefCell::new(Vec::new()),
    };
    let prepared = PreparedUninstall::prepare(
        root.path(),
        &HashMap::new(),
        &SystemdManager::new(&runner),
        &runner,
        &pontia_clients::integrations(),
    )
    .unwrap();
    assert!(prepared.summary().is_empty());
    prepared
        .uninstall(&SystemdManager::new(&runner), &runner)
        .unwrap();
    assert!(runner.calls.borrow().is_empty());
}

#[test]
fn local_pi_source_and_external_claude_host_are_detached_without_deleting_source_files() {
    let fixture = Fixture::new();
    let source = fixture.home.join("plugin-source");
    write(
        &source.join("package.json"),
        r#"{"name":"@pontia/pi-client-plugin"}"#,
    );
    write(&source.join("index.ts"), "export default () => {};");
    write(
        &fixture.home.join(".pi/agent/settings.json"),
        &serde_json::json!({"packages": [source]}).to_string(),
    );
    let host = fixture.home.join("external-claude/main.js");
    write(&host, "external host");
    let prepared = PreparedUninstall::prepare(
        &fixture.home,
        &HashMap::from([("PONTIA_CLAUDE_HOST_PATH".into(), host.display().to_string())]),
        &SystemdManager::new(&fixture.runner),
        &fixture.runner,
        &pontia_clients::integrations(),
    )
    .unwrap();
    prepared
        .uninstall(&SystemdManager::new(&fixture.runner), &fixture.runner)
        .unwrap();
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(fixture.home.join(".pi/agent/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings["packages"], serde_json::json!([]));
    assert_eq!(
        fs::read_to_string(source.join("index.ts")).unwrap(),
        "export default () => {};"
    );
    assert_eq!(fs::read_to_string(host).unwrap(), "external host");
    assert!(!fixture.pontia.join("clients/claude-client").exists());
}

#[test]
fn execution_check_uses_installed_service_configuration_instead_of_shell_overrides() {
    let fixture = Fixture::new();
    write(
        &fixture.pontia.join("config.toml"),
        "bind_addr = '127.0.0.1:8123'\nexternal_api_token = 'installed-token'\n",
    );
    let prepared = PreparedUninstall::prepare(
        &fixture.home,
        &HashMap::from([
            (
                "PONTIA_HOME".into(),
                fixture.home.join("other-instance").display().to_string(),
            ),
            ("PONTIA_EXTERNAL_API_TOKEN".into(), "shell-token".into()),
        ]),
        &SystemdManager::new(&fixture.runner),
        &fixture.runner,
        &pontia_clients::integrations(),
    )
    .unwrap();
    let config = prepared.service_config().unwrap();
    assert_eq!(config.pontia_home, fixture.pontia);
    assert_eq!(config.bind_addr, "127.0.0.1:8123".parse().unwrap());
    assert_eq!(
        config.external_api_token.as_deref(),
        Some("installed-token")
    );
}
