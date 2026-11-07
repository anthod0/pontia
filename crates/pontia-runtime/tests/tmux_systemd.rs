#![cfg(target_os = "linux")]

use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use pontia_runtime::spawn_tmux_session;

// Re-entered by the transient test service, so the real launch path runs inside
// a service with the same cgroup cleanup policy as pontiad.
#[test]
fn service_process() {
    let Some(root) = env::var_os("PONTIA_TMUX_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let pid = std::process::id();
    assert!(
        spawn_tmux_session(
            &format!("managed_{pid}"),
            &root,
            "value='literal $value with spaces'; printf '%s' \"$value\" > launched; exec sleep 300",
        )
        .unwrap()
        .success()
    );
    fs::write(root.join("ready"), pid.to_string()).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

struct Fixture {
    root: tempfile::TempDir,
    unit: String,
    tmux: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let unit = format!(
            "pontia-tmux-test-{}.service",
            root.path().file_name().unwrap().to_str().unwrap()
        );
        let tmux = env::split_paths(&env::var_os("PATH").unwrap())
            .map(|directory| directory.join("tmux"))
            .find(|path| path.is_file())
            .expect("tmux must be installed");
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let wrapper = bin.join("tmux");
        fs::write(
            &wrapper,
            "#!/bin/sh\nexec \"$PONTIA_TEST_TMUX\" -f /dev/null -S \"$PONTIA_TEST_SOCKET\" \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root, unit, tmux }
    }

    fn tmux(&self, args: &[&str]) -> Output {
        Command::new(&self.tmux)
            .env("SHELL", "/bin/sh")
            .args(["-f", "/dev/null", "-S"])
            .arg(self.root.path().join("tmux.sock"))
            .args(args)
            .output()
            .unwrap()
    }

    fn start(&self) -> String {
        let path = env::join_paths(
            std::iter::once(self.root.path().join("bin"))
                .chain(env::split_paths(&env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let output = Command::new("systemd-run")
            .args([
                "--user",
                "--quiet",
                "--collect",
                "--property=Type=exec",
                "--property=KillMode=control-group",
                "--unit",
                &self.unit,
            ])
            .arg(format!("--setenv=PATH={}", path.to_str().unwrap()))
            .arg(format!(
                "--setenv=PONTIA_TMUX_TEST_ROOT={}",
                self.root.path().display()
            ))
            .arg(format!("--setenv=PONTIA_TEST_TMUX={}", self.tmux.display()))
            .arg(format!(
                "--setenv=PONTIA_TEST_SOCKET={}",
                self.root.path().join("tmux.sock").display()
            ))
            .arg("--setenv=TMUX=")
            .arg("--setenv=SHELL=/bin/sh")
            .arg("--")
            .arg(env::current_exe().unwrap())
            .args(["--exact", "service_process", "--nocapture"])
            .output()
            .unwrap();
        assert_success(&output);
        self.wait_for_launch("")
    }

    fn wait_for_launch(&self, previous: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(pid) = fs::read_to_string(self.root.path().join("ready"))
                && !pid.is_empty()
                && pid != previous
                && fs::read_to_string(self.root.path().join("launched"))
                    .is_ok_and(|value| value == "literal $value with spaces")
            {
                return pid;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let journal = Command::new("journalctl")
            .args(["--user", "-u", &self.unit, "--no-pager", "-n", "30"])
            .output()
            .unwrap();
        panic!(
            "service did not launch tmux: {}",
            String::from_utf8_lossy(&journal.stdout)
        );
    }

    fn service(&self, action: &str) -> Output {
        Command::new("systemctl")
            .args(["--user", action, &self.unit])
            .output()
            .unwrap()
    }

    fn identity(&self, session: &str) -> String {
        let output = self.tmux(&["display-message", "-p", "-t", session, "#{pid}:#{pane_pid}"]);
        assert_success(&output);
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this fixture's randomly named service and private socket.
        let _ = self.service("stop");
        let _ = self.tmux(&["kill-server"]);
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires a running systemd user manager and tmux"]
fn tmux_sessions_survive_service_restart_and_stop() {
    for user_server_first in [false, true] {
        let fixture = Fixture::new();
        if user_server_first {
            assert_success(&fixture.tmux(&["new-session", "-d", "-s", "user", "sleep 300"]));
        }
        let first_pid = fixture.start();
        if !user_server_first {
            assert_success(&fixture.tmux(&["new-session", "-d", "-s", "user", "sleep 300"]));
        }
        let managed = format!("managed_{first_pid}");
        let managed_identity = fixture.identity(&managed);
        let user_identity = fixture.identity("user");

        assert_success(&fixture.service("restart"));
        fixture.wait_for_launch(&first_pid);
        assert_eq!(fixture.identity(&managed), managed_identity);
        assert_eq!(fixture.identity("user"), user_identity);

        assert_success(&fixture.service("stop"));
        assert_eq!(fixture.identity(&managed), managed_identity);
        assert_eq!(fixture.identity("user"), user_identity);
    }
}
