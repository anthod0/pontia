use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use std::process::{Child, Command, Stdio};

struct TestProcess(Child);
impl Drop for TestProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn fingerprint_tracks_a_process_without_a_terminal_and_rejects_reused_identity() {
    let mut child = TestProcess(
        Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let fingerprint = ProcessFingerprint::capture(child.0.id()).unwrap();
    assert_eq!(fingerprint.observe(), ProcessObservation::Alive);
    let mut reused_pid = fingerprint.clone();
    reused_pid.start_time_seconds -= 1;
    assert_eq!(reused_pid.observe(), ProcessObservation::Exited);
    let mut previous_boot = fingerprint.clone();
    previous_boot.boot_time_seconds -= 1;
    assert_eq!(previous_boot.observe(), ProcessObservation::Exited);
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert_eq!(fingerprint.observe(), ProcessObservation::Exited);
}
