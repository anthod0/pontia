#[path = "../src/diagnostics.rs"]
mod diagnostics;
#[allow(dead_code)]
#[path = "../src/login.rs"]
mod login;
#[allow(dead_code)]
#[path = "../src/remote.rs"]
mod remote;

use pontia::{
    lifecycle::ServiceManager,
    manager::{CommandOutput, CommandRunner, SystemdManager},
};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::TcpListener,
    thread,
};

struct Runner {
    running: bool,
    unavailable: bool,
}
impl CommandRunner for Runner {
    fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, String> {
        assert_eq!(program, "systemctl");
        if self.unavailable {
            return Err("service manager unavailable".into());
        }
        let stdout = match args[1].as_str() {
            "is-enabled" => "enabled".into(),
            "show" => format!(
                "LoadState=loaded\nActiveState={}\n",
                if self.running { "active" } else { "inactive" }
            ),
            _ => panic!("diagnostics must not change the service"),
        };
        Ok(CommandOutput {
            code: 0,
            stdout,
            stderr: String::new(),
        })
    }
}

#[tokio::test]
async fn diagnoses_the_installed_service_home_without_modifying_files() {
    for (running, http_ok, unavailable) in [
        (true, true, false),
        (false, true, false),
        (true, false, false),
        (true, true, true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("installed-pontia");
        fs::create_dir(&home).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = format!("bind_addr = \"{}\"\n", listener.local_addr().unwrap());
        fs::write(home.join("config.toml"), &config).unwrap();
        let request = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut bytes = [0; 2048];
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let n = stream.read(&mut bytes).unwrap();
                assert_ne!(n, 0);
                request.extend_from_slice(&bytes[..n]);
            }
            assert!(String::from_utf8_lossy(&request).starts_with("GET /healthz "));
            let body = if http_ok {
                r#"{"status":"ok"}"#
            } else {
                r#"{"status":"failed"}"#
            };
            write!(stream, "HTTP/1.0 200 OK\r\n\r\n{body}").unwrap();
        });
        let runner = Runner {
            running,
            unavailable,
        };
        let manager = SystemdManager::new(&runner);
        let path = manager.definition_path(root.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let definition = pontia::definition::render_systemd(
            &root.path().join("pontiad"),
            &home,
            "https://pontia.dev",
        )
        .unwrap();
        fs::write(&path, &definition).unwrap();
        let vars = HashMap::from([(
            "PONTIA_HOME".into(),
            root.path().join("wrong-home").display().to_string(),
        )]);
        let healthy = diagnostics::run(&manager, root.path(), &vars, true)
            .await
            .unwrap();
        assert_eq!(healthy, running && http_ok && !unavailable);
        request.join().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), definition);
        assert_eq!(
            fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
        assert!(!home.join("auth.json").exists());
        assert!(!root.path().join("wrong-home").exists());
    }
}

#[tokio::test]
async fn invalid_configuration_and_absent_service_are_reported_without_creating_files() {
    let root = tempfile::tempdir().unwrap();
    let runner = Runner {
        running: false,
        unavailable: true,
    };
    let manager = SystemdManager::new(&runner);
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    fs::write(root.path().join("config.toml"), "not valid toml!").unwrap();
    assert!(
        !diagnostics::run(&manager, root.path(), &vars, true)
            .await
            .unwrap()
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn absent_and_malformed_credentials_do_not_attempt_cloud_access() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([
        ("PONTIA_HOME".into(), root.path().display().to_string()),
        ("PONTIA_AUTH_ORIGIN".into(), "invalid-origin".into()),
    ]);
    assert!(login::account(&vars).await.unwrap().is_none());
    fs::write(root.path().join("auth.json"), r#"{"token":"invalid"}"#).unwrap();
    assert!(login::account(&vars).await.is_err());
    assert!(matches!(
        remote::registration_status(&vars).await.unwrap(),
        remote::RegistrationStatus::Disabled
    ));
}
