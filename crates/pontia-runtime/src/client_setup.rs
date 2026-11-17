use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupOperation {
    pub id: String,
    pub client_type: String,
    pub client_key: String,
    pub status: SetupStatus,
    pub error: Option<String>,
    #[serde(default)]
    pub output: String,
}

#[derive(Serialize, Deserialize)]
struct StoredOperation {
    #[serde(flatten)]
    operation: SetupOperation,
    worker: crate::process::ProcessFingerprint,
}

fn directory(home: &Path) -> PathBuf {
    home.join("state/client-setup")
}

fn open_lock(home: &Path) -> Result<File, String> {
    fs::create_dir_all(directory(home)).map_err(|error| error.to_string())?;
    private_file(&directory(home).join("lock"), false)
}

fn private_file(path: &Path, truncate: bool) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(truncate)
        .mode(0o600)
        .open(path)
        .map_err(|error| error.to_string())
}

fn save(home: &Path, operation: &StoredOperation) -> Result<(), String> {
    let path = directory(home).join("result.tmp");
    let mut file = private_file(&path, true)?;
    serde_json::to_writer(&mut file, operation).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(path, directory(home).join("result.json")).map_err(|error| error.to_string())
}

/// Read the latest receipt and log, including after pontiad has restarted.
pub fn latest(home: &Path) -> Result<Option<SetupOperation>, String> {
    let bytes = match fs::read(directory(home).join("result.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let stored: StoredOperation =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let mut operation = stored.operation;
    if operation.status == SetupStatus::Running
        && stored.worker.observe() == crate::process::ProcessObservation::Exited
    {
        let current: StoredOperation = serde_json::from_slice(
            &fs::read(directory(home).join("result.json")).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if current.operation.id == operation.id && current.operation.status == SetupStatus::Running
        {
            operation.status = SetupStatus::Failed;
            operation.error =
                Some("Setup stopped before reporting a result. You can run setup again.".into());
        } else {
            operation = current.operation;
        }
    }
    let mut log = File::open(directory(home).join(format!("{}.log", operation.id)))
        .map_err(|error| error.to_string())?;
    // Keep polling responses bounded while retaining the complete log on the device.
    let length = log.metadata().map_err(|error| error.to_string())?.len();
    log.seek(SeekFrom::Start(length.saturating_sub(64 * 1024)))
        .map_err(|error| error.to_string())?;
    let mut output = Vec::new();
    log.read_to_end(&mut output)
        .map_err(|error| error.to_string())?;
    operation.output = String::from_utf8_lossy(&output).into_owned();
    Ok(Some(operation))
}

/// Start the CLI outside pontiad's service lifetime and wait only for its startup receipt.
pub fn start(home: &Path, cli: &Path, client: &str, id: &str) -> Result<SetupOperation, String> {
    let mut command = launcher(cli)?;
    command
        .args(["setup-worker", client, id])
        .env("PONTIA_HOME", home)
        .env("NO_COLOR", "1")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Failed to start setup: {error}"))?;
    let mut receipt = String::new();
    let read = BufReader::new(child.stdout.take().expect("piped stdout")).read_line(&mut receipt);
    if let Err(error) = read {
        std::thread::spawn(move || child.wait_with_output());
        return Err(format!("Failed to read setup receipt: {error}"));
    }
    if receipt.is_empty() {
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        return Err(format!(
            "Failed to start setup: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    std::thread::spawn(move || child.wait_with_output());
    serde_json::from_str::<Result<SetupOperation, String>>(&receipt)
        .map_err(|error| format!("Invalid setup receipt: {error}"))?
}

fn launcher(cli: &Path) -> Result<Command, String> {
    #[cfg(target_os = "linux")]
    if Path::new("/run/systemd/system").is_dir() {
        let mut command = Command::new("systemd-run");
        let help = Command::new("systemd-run")
            .arg("--help")
            .output()
            .map_err(|error| error.to_string())?;
        if !help.status.success() {
            return Err(String::from_utf8_lossy(&help.stderr).into_owned());
        }
        if String::from_utf8_lossy(&help.stdout).contains("--expand-environment=") {
            command.arg("--expand-environment=no");
        }
        command.args([
            "--user",
            "--scope",
            "--quiet",
            "--collect",
            "--no-ask-password",
            "--description=Pontia client setup",
            "--",
        ]);
        command.arg(cli);
        return Ok(command);
    }
    {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new(cli);
        // Separate from launchd's service process group, or a container's foreground group.
        command.process_group(0);
        Ok(command)
    }
}

/// Called only by the hidden CLI worker. All installation behavior remains in `pontia setup`.
pub fn run_worker(
    home: &Path,
    cli: &Path,
    client: &str,
    key: &str,
    setup_name: &str,
    id: &str,
    receipt: &mut impl Write,
) -> Result<(), String> {
    let lock = open_lock(home)?;
    lock.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => "Another client setup is already running.".to_string(),
        TryLockError::Error(error) => error.to_string(),
    })?;
    let worker = crate::process::ProcessFingerprint::capture(std::process::id())
        .ok_or("Cannot identify the setup worker")?;
    let log = private_file(&directory(home).join(format!("{id}.log")), true)?;
    let mut operation = SetupOperation {
        id: id.into(),
        client_type: client.into(),
        client_key: key.into(),
        status: SetupStatus::Running,
        error: None,
        output: String::new(),
    };
    let mut stored = StoredOperation {
        operation: operation.clone(),
        worker,
    };
    save(home, &stored)?;
    serde_json::to_writer(&mut *receipt, &Ok::<_, String>(&operation))
        .map_err(|error| error.to_string())?;
    writeln!(receipt)
        .and_then(|_| receipt.flush())
        .map_err(|error| error.to_string())?;
    let result = Command::new(cli)
        .args(["setup", setup_name])
        .env("PONTIA_HOME", home)
        .env("NO_COLOR", "1")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|error| error.to_string())?)
        .stderr(log)
        .status();
    match result {
        Ok(status) if status.success() => operation.status = SetupStatus::Succeeded,
        result => {
            operation.status = SetupStatus::Failed;
            operation.error = Some(match result {
                Ok(status) => format!("Setup exited with {status}."),
                Err(error) => format!("Failed to execute setup: {error}"),
            });
        }
    }
    stored.operation = operation;
    save(home, &stored)
}
