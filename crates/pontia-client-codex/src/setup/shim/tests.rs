use super::*;
use std::{os::unix::fs::symlink, process::Command};

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
    native: PathBuf,
    vars: HashMap<String, String>,
}

impl Fixture {
    fn new(shell: &str) -> Self {
        let test_root = tempfile::tempdir().unwrap();
        let home = test_root.path().join("user");
        let root = test_root.path().join("pontia ' home");
        let native = test_root.path().join("native bin");
        fs::create_dir(&home).unwrap();
        fs::create_dir(&native).unwrap();
        executable(&native.join("pontia"), "#!/bin/sh\nexit 0\n");
        executable(&native.join("codex"), "#!/bin/sh\nprintf '%s\\0' \"$@\"\n");
        let vars = HashMap::from([
            ("PONTIA_HOME".into(), root.display().to_string()),
            ("SHELL".into(), format!("/bin/{shell}")),
            (
                "PATH".into(),
                std::env::join_paths([
                    root.join("bin"),
                    native.clone(),
                    PathBuf::from("/usr/bin"),
                    PathBuf::from("/bin"),
                ])
                .unwrap()
                .into_string()
                .unwrap(),
            ),
        ]);
        Self {
            _root: test_root,
            home,
            root,
            native,
            vars,
        }
    }

    fn enable(&self) -> ShimSetup {
        let setup = ShimSetup::inspect(&self.vars, &self.home).unwrap();
        setup.install().unwrap();
        setup
    }

    fn args(&self, args: &[&str]) -> Vec<String> {
        self.args_in(&self.home, args)
    }

    fn args_in(&self, cwd: &Path, args: &[&str]) -> Vec<String> {
        let output = Command::new(self.root.join("bin/codex"))
            .args(args)
            .current_dir(cwd)
            .env("PATH", &self.vars["PATH"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|value| !value.is_empty())
            .map(|value| String::from_utf8(value.to_vec()).unwrap())
            .collect()
    }
}

fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn routes_tui_entries_preserving_argument_boundaries_and_explicit_choices() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    for args in [
        vec![],
        vec!["agents"],
        vec!["resume", "--last"],
        vec!["resume", "--last", "--include-non-interactive"],
        vec!["fork", "session id"],
        vec!["-m", "some model", "prompt ' with $ symbols"],
        vec!["-c", "model=\"agents\"", "resume", "--all"],
        vec!["--model=local", "--", "--remote"],
        vec!["-m", "--remote", "a prompt"],
        vec!["-m", "--cd", "a prompt"],
        vec!["--", "--cd=/prompt"],
    ] {
        let mut expected = vec![
            "--remote".to_string(),
            format!("unix://{}/codex-proxy.sock", fixture.root.display()),
            "--cd".to_string(),
            fixture.home.display().to_string(),
        ];
        expected.extend(args.iter().map(|arg| arg.to_string()));
        assert_eq!(fixture.args(&args), expected, "{args:?}");
    }
    for args in [
        vec!["--remote", "unix:///explicit.sock", "agents"],
        vec!["agents", "--remote=unix:///chosen.sock"],
        vec!["resume", "--no-daemon"],
        vec!["--help"],
        vec!["--version"],
        vec!["agents", "-h"],
        vec!["login"],
        vec!["update"],
        vec!["mcp", "list"],
        vec!["app-server", "daemon", "start"],
        vec!["exec", "hello"],
        vec!["-c", "model=\"test\"", "e", "hello"],
        vec!["help", "resume"],
    ] {
        assert_eq!(fixture.args(&args), args, "{args:?}");
    }
}

#[test]
fn proxied_tui_uses_each_invocations_working_directory() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    for cwd in [&fixture.root, &fixture.native] {
        assert_eq!(
            fixture.args_in(cwd, &[]),
            [
                "--remote".to_string(),
                format!("unix://{}/codex-proxy.sock", fixture.root.display()),
                "--cd".to_string(),
                cwd.display().to_string(),
            ]
        );
    }
}

#[test]
fn proxied_tui_preserves_explicit_working_directory_without_duplicate_options() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    for args in [
        vec!["-m", "some model", "-C", "/work space", "prompt"],
        vec!["--cd", "../relative workspace"],
        vec!["--cd=/work space", "agents"],
        vec!["-C/workspace", "agents"],
        vec!["-C=/workspace"],
        vec!["resume", "--last", "--cd", "/workspace"],
        vec!["fork", "session id", "-C/workspace"],
    ] {
        let mut expected = vec![
            "--remote".to_string(),
            format!("unix://{}/codex-proxy.sock", fixture.root.display()),
        ];
        expected.extend(args.iter().map(|arg| arg.to_string()));
        assert_eq!(fixture.args(&args), expected, "{args:?}");
    }
}

#[test]
fn repeated_setup_repairs_missing_launcher_and_path_preserving_shell_content() {
    let fixture = Fixture::new("bash");
    let rc = fixture.home.join(".bashrc");
    let original = "export USER_SETTING=kept\n";
    fs::write(&rc, original).unwrap();
    let setup = fixture.enable();
    let once = fs::read_to_string(&rc).unwrap();
    let files = [
        rc.clone(),
        fixture.root.join("bin/codex"),
        fixture.root.join("env.sh"),
        fixture.root.join("state/codex-shim.json"),
    ];
    let modified = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(123456);
    for path in &files {
        fs::File::open(path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
    }
    setup.install().unwrap();
    for path in &files {
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
    }
    assert_eq!(fs::read_to_string(&rc).unwrap(), once);
    let resolve = || {
        Command::new("/bin/bash")
            .args([
                "--noprofile",
                "--norc",
                "-c",
                ". \"$1\"; . \"$1\"; command -v codex",
                "test",
            ])
            .arg(&rc)
            .env("PATH", fixture.native.as_os_str())
            .output()
            .unwrap()
    };
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.root.join("bin/codex").to_str().unwrap()
    );
    let output = Command::new("/bin/bash")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            ". \"$1\"; . \"$1\"; printf '%s' \"$PATH\"",
            "test",
        ])
        .arg(&rc)
        .env(
            "PATH",
            std::env::join_paths([
                fixture.native.clone(),
                fixture.root.join("bin"),
                PathBuf::from("/bin"),
            ])
            .unwrap(),
        )
        .output()
        .unwrap();
    let actual = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        std::env::split_paths(&actual).collect::<Vec<_>>(),
        vec![
            fixture.root.join("bin"),
            fixture.native.clone(),
            PathBuf::from("/bin")
        ]
    );
    fs::remove_file(fixture.root.join("bin/codex")).unwrap();
    fs::remove_file(fixture.root.join("env.sh")).unwrap();
    assert!(
        Command::new("/bin/bash")
            .args(["--noprofile", "--norc", "-ec", ". \"$1\"", "test"])
            .arg(&rc)
            .status()
            .unwrap()
            .success()
    );
    ShimSetup::inspect(&fixture.vars, &fixture.home)
        .unwrap()
        .install()
        .unwrap();
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.root.join("bin/codex").to_str().unwrap()
    );
    fs::write(&rc, original).unwrap();
    ShimSetup::inspect(&fixture.vars, &fixture.home)
        .unwrap()
        .install()
        .unwrap();
    assert_eq!(fs::read_to_string(&rc).unwrap(), once);
    assert_eq!(fixture.args(&["--version"]), ["--version"]);
    let override_path = format!(
        "export PATH={}:\"$PATH\"\n",
        quote(fixture.native.to_str().unwrap())
    );
    fs::write(&rc, format!("{once}{override_path}")).unwrap();
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.native.join("codex").to_str().unwrap()
    );
    setup.install().unwrap();
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.root.join("bin/codex").to_str().unwrap()
    );
    assert!(fs::read_to_string(&rc).unwrap().contains(&override_path));
}

#[test]
fn doctor_warns_when_the_installed_launcher_directory_is_missing_or_not_first() {
    let mut fixture = Fixture::new("bash");
    let integration = crate::setup::integration();
    let bin = fixture.root.join("bin");
    fixture.vars.remove("PATH");
    assert!(
        integration
            .setup_warning(&fixture.root, &fixture.vars)
            .is_none()
    );
    fixture.enable();
    assert!(
        integration
            .setup_warning(&fixture.root, &fixture.vars)
            .is_some()
    );
    for (path, expect_warning) in [
        (vec![&fixture.native], true),
        (vec![&fixture.native, &bin], true),
        (vec![&bin, &fixture.native], false),
    ] {
        fixture.vars.insert(
            "PATH".into(),
            std::env::join_paths(path).unwrap().into_string().unwrap(),
        );
        assert_eq!(
            integration
                .setup_warning(&fixture.root, &fixture.vars)
                .is_some(),
            expect_warning
        );
    }
}

#[test]
fn native_upgrades_and_an_unavailable_proxy_do_not_change_launch_behavior() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    let native = fixture.native.join("codex");
    fs::remove_file(&native).unwrap();
    let version = fixture.native.join("version one");
    executable(&version, "#!/bin/sh\nexit 41\n");
    symlink(&version, &native).unwrap();
    let launch = || {
        Command::new(fixture.root.join("bin/codex"))
            .env("PATH", &fixture.vars["PATH"])
            .status()
            .unwrap()
    };
    assert_eq!(launch().code(), Some(41));
    fs::remove_file(&native).unwrap();
    let version = fixture.native.join("version two");
    executable(&version, "#!/bin/sh\nexit 42\n");
    symlink(&version, &native).unwrap();
    assert_eq!(launch().code(), Some(42));
    struct Runner;
    impl pontia_runtime::local_service::CommandRunner for Runner {
        fn run(
            &self,
            _: &str,
            _: &[String],
        ) -> Result<pontia_runtime::local_service::CommandOutput, String> {
            Ok(pontia_runtime::local_service::CommandOutput {
                code: 0,
                stdout: if cfg!(target_os = "macos") {
                    "501"
                } else {
                    "test"
                }
                .into(),
                stderr: String::new(),
            })
        }
        fn run_with_env(
            &self,
            program: &str,
            args: &[String],
            _: &[(String, String)],
        ) -> Result<pontia_runtime::local_service::CommandOutput, String> {
            self.run(program, args)
        }
    }
    let setup = crate::setup::inspect(&fixture.vars, &fixture.home, &Runner).unwrap();
    assert_eq!(setup.executable, native);
}

#[test]
fn setup_refuses_to_overwrite_native_installations() {
    let fixture = Fixture::new("bash");
    let setup = ShimSetup::inspect(&fixture.vars, &fixture.home).unwrap();
    fs::create_dir_all(fixture.root.join("bin")).unwrap();
    let path = fixture.root.join("bin/codex");
    executable(&path, "#!/bin/sh\nexit 7\n");
    assert!(setup.install().is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "#!/bin/sh\nexit 7\n");
}

#[test]
fn shell_selection_respects_zdotdir_and_fish_config_location() {
    for (shell, variable, directory, files) in [
        ("zsh", "ZDOTDIR", "zsh", vec![".zshrc", ".zprofile"]),
        (
            "fish",
            "XDG_CONFIG_HOME",
            "config",
            vec!["fish/config.fish"],
        ),
    ] {
        let mut fixture = Fixture::new(shell);
        let config = fixture.home.join(directory);
        fixture
            .vars
            .insert(variable.into(), config.display().to_string());
        let setup = fixture.enable();
        for file in &files {
            assert!(config.join(file).is_file());
        }
        setup.install().unwrap();
        for file in files {
            assert_eq!(
                fs::read_to_string(config.join(file))
                    .unwrap()
                    .matches(BEGIN)
                    .count(),
                1
            );
        }
    }
}

#[test]
#[ignore = "requires installed fish"]
fn fish_startup_prioritizes_launcher_after_existing_user_path_configuration() {
    let fixture = Fixture::new("fish");
    let config = fixture.home.join(".config");
    fs::create_dir_all(config.join("fish")).unwrap();
    fs::write(
        config.join("fish/config.fish"),
        format!(
            "fish_add_path --path {}\n",
            quote(fixture.native.to_str().unwrap())
        ),
    )
    .unwrap();
    let setup = fixture.enable();
    let resolve = || {
        Command::new("fish")
            .args(["--private", "-c", "command -s codex"])
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", fixture.home.join("data"))
            .env("XDG_CACHE_HOME", fixture.home.join("cache"))
            .env("PATH", &fixture.vars["PATH"])
            .output()
            .unwrap()
    };
    let output = resolve();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        fixture.root.join("bin/codex").to_str().unwrap()
    );
    let rc = config.join("fish/config.fish");
    let mut contents = fs::read_to_string(&rc).unwrap();
    contents.push_str(&format!(
        "set -gx PATH {} $PATH\n",
        quote(fixture.native.to_str().unwrap())
    ));
    fs::write(&rc, contents).unwrap();
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.native.join("codex").to_str().unwrap()
    );
    setup.install().unwrap();
    assert_eq!(
        String::from_utf8(resolve().stdout).unwrap().trim(),
        fixture.root.join("bin/codex").to_str().unwrap()
    );
}

#[test]
fn inactive_or_unavailable_workspace_runs_native_codex_unchanged() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    for exit in [1, 127] {
        executable(
            &fixture.native.join("pontia"),
            &format!("#!/bin/sh\nexit {exit}\n"),
        );
        for args in [
            vec![],
            vec!["resume", "--last"],
            vec!["fork", "native-id"],
            vec!["-C", "/unregistered", "prompt"],
        ] {
            assert_eq!(fixture.args(&args), args);
        }
    }
}

#[test]
fn launcher_checks_the_effective_directory_with_argument_boundaries_preserved() {
    let fixture = Fixture::new("bash");
    fixture.enable();
    let log = fixture.root.join("check-args");
    executable(
        &fixture.native.join("pontia"),
        &format!(
            "#!/bin/sh\nprintf '%s\\0' \"$@\" > {}\nexit 1\n",
            quote(log.to_str().unwrap())
        ),
    );
    for (args, cwd) in [
        (vec![], fixture.home.to_str().unwrap()),
        (vec!["--cd", "../project ' $name"], "../project ' $name"),
        (
            vec!["--cd=/project space", "resume", "--last"],
            "/project space",
        ),
        (vec!["-C/project", "fork", "id"], "/project"),
        (vec!["-C=/project"], "/project"),
        (vec!["--", "--cd=/prompt"], fixture.home.to_str().unwrap()),
    ] {
        fixture.args(&args);
        assert_eq!(
            fs::read(&log).unwrap(),
            format!("workspace-check\0--\0{cwd}\0").as_bytes()
        );
    }
}

#[tokio::test]
async fn detects_installed_artifacts_and_partial_codex_setup() {
    use pontia_agent_control::client_contract::{ClientSetup, DetectionContext};
    let mut fixture = Fixture::new("bash");
    fixture
        .vars
        .insert("HOME".into(), fixture.home.display().to_string());
    let context = DetectionContext {
        vars: &fixture.vars,
        pontia_home: &fixture.root,
    };
    let integration = crate::setup::integration();
    assert!(matches!(
        integration.detect_setup(&context).await.unwrap(),
        ClientSetup::Required(_)
    ));
    fixture.enable();
    assert!(matches!(
        integration.detect_setup(&context).await.unwrap(),
        ClientSetup::Required(_)
    ));
    let service = crate::setup::codex_service_path(&fixture.home);
    fs::create_dir_all(service.parent().unwrap()).unwrap();
    let render = if cfg!(target_os = "macos") {
        crate::setup::render_codex_launchd
    } else {
        crate::setup::render_codex_systemd
    };
    fs::write(
        service,
        render(&fixture.native.join("codex"), &fixture.home.join(".codex")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        integration.detect_setup(&context).await.unwrap(),
        ClientSetup::Initialized
    );
    fs::write(fixture.home.join(".bashrc"), "# user config").unwrap();
    assert!(matches!(
        integration.detect_setup(&context).await.unwrap(),
        ClientSetup::Required(_)
    ));
}
