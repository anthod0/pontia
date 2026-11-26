use crate::process::{ProcessFingerprint, ProcessObservation};
use pontia_core::private_fs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Read, Seek, SeekFrom, Write},
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

mod recipes;
pub use recipes::{PackageManager, ScriptSource};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preflight,
    Installing,
    Verifying,
    Done,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallState {
    pub id: Uuid,
    pub client_type: String,
    pub client_key: String,
    pub method_id: String,
    pub phase: Phase,
    pub status: Status,
    pub cancel_requested: bool,
    pub error: Option<String>,
    worker: ProcessFingerprint,
}

fn directory(home: &Path) -> PathBuf {
    home.join("state/client-operations")
}

fn lock(dir: &Path, name: &str) -> io::Result<File> {
    private_fs::open(
        &dir.join(name),
        OpenOptions::new().create(true).truncate(false).write(true),
    )
}

// Serialize cancellation, phase transitions and crash recovery across processes.
fn state_lock(dir: &Path) -> io::Result<File> {
    let file = lock(dir, "state.lock")?;
    file.lock()?;
    Ok(file)
}

fn save(dir: &Path, state: &InstallState) -> io::Result<()> {
    let mut file = NamedTempFile::new_in(dir)?;
    serde_json::to_writer(&mut file, state)?;
    file.as_file().sync_all()?;
    file.persist(dir.join(format!("{}.json", state.id)))?;
    Ok(())
}

fn read_state(dir: &Path, path: &Path) -> io::Result<InstallState> {
    let mut state: InstallState = serde_json::from_reader(File::open(path)?)?;
    if state.status != Status::Running {
        return Ok(state);
    }
    let lease = lock(dir, "lock")?;
    let abandoned = match lease.try_lock() {
        Ok(()) => true,
        Err(TryLockError::WouldBlock) => {
            fs::read_link(dir.join("latest.json"))? != Path::new(&format!("{}.json", state.id))
        }
        Err(TryLockError::Error(error)) => return Err(error),
    };
    if abandoned || state.worker.observe() == ProcessObservation::Exited {
        state.status = Status::Failed;
        state.error = Some("Install worker exited before reporting a result".into());
        save(dir, &state)?;
    }
    Ok(state)
}

pub fn latest(home: &Path) -> io::Result<Option<InstallState>> {
    let dir = directory(home);
    if !dir.try_exists()? {
        return Ok(None);
    }
    let _guard = state_lock(&dir)?;
    match read_state(&dir, &dir.join("latest.json")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        other => other.map(Some),
    }
}

pub fn state(home: &Path, id: Uuid) -> io::Result<InstallState> {
    let dir = directory(home);
    let _guard = state_lock(&dir)?;
    read_state(&dir, &dir.join(format!("{id}.json")))
}

pub fn cancel(home: &Path, id: Uuid) -> io::Result<()> {
    let dir = directory(home);
    let _guard = state_lock(&dir)?;
    let mut state = read_state(&dir, &dir.join(format!("{id}.json")))?;
    if state.status != Status::Running {
        return Err(io::Error::other("Install has finished"));
    }
    state.cancel_requested = true;
    save(&dir, &state)
}

pub fn read_log(home: &Path, id: Uuid, offset: u64) -> io::Result<Vec<u8>> {
    let mut file = File::open(directory(home).join(format!("{id}.log")))?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.take(64 * 1024).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub struct InstallOperation {
    home: PathBuf,
    dir: PathBuf,
    state: InstallState,
    log: File,
}

impl InstallOperation {
    pub fn pontia_home(&self) -> &Path {
        &self.home
    }

    pub fn id(&self) -> Uuid {
        self.state.id
    }

    pub fn log(&mut self, message: &str) -> io::Result<()> {
        writeln!(self.log, "{message}")
    }

    fn requested(&self) -> io::Result<bool> {
        let state: InstallState =
            serde_json::from_reader(File::open(self.dir.join(format!("{}.json", self.id())))?)?;
        Ok(state.cancel_requested)
    }

    pub fn checkpoint(&self) -> io::Result<()> {
        if self.requested()? {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Install cancelled",
            ));
        }
        Ok(())
    }

    pub fn advance(&mut self, next: Phase) -> io::Result<()> {
        let _guard = state_lock(&self.dir)?;
        self.checkpoint()?;
        if !matches!(
            (self.state.phase, next),
            (Phase::Preflight, Phase::Installing) | (Phase::Installing, Phase::Verifying)
        ) {
            return Err(io::Error::other("Invalid install phase transition"));
        }
        self.state.phase = next;
        save(&self.dir, &self.state)
    }
}

/// Runs on a blocking worker thread or in the foreground CLI process.
pub fn run(
    home: &Path,
    client_type: &str,
    client_key: &str,
    method_id: &str,
    install: impl FnOnce(&mut InstallOperation) -> io::Result<()>,
) -> io::Result<InstallState> {
    let dir = directory(home);
    private_fs::directory(&dir)?;
    let exclusive = lock(&dir, "lock")?;
    exclusive.try_lock()?;
    let worker = ProcessFingerprint::capture(std::process::id())
        .ok_or_else(|| io::Error::other("Cannot identify install worker"))?;
    let state = InstallState {
        id: Uuid::now_v7(),
        client_type: client_type.into(),
        client_key: client_key.into(),
        method_id: method_id.into(),
        phase: Phase::Preflight,
        status: Status::Running,
        cancel_requested: false,
        error: None,
        worker,
    };
    let log = private_fs::append_log(&dir.join(format!("{}.log", state.id)))?;
    {
        let _guard = state_lock(&dir)?;
        save(&dir, &state)?;
        let staging = tempfile::tempdir_in(&dir)?;
        let link = staging.path().join("latest");
        symlink(format!("{}.json", state.id), &link)?;
        fs::rename(link, dir.join("latest.json"))?;
        prune_history(&dir, state.id)?;
    }
    let mut operation = InstallOperation {
        home: home.into(),
        dir,
        state,
        log,
    };
    let result = install(&mut operation);
    let _guard = state_lock(&operation.dir)?;
    operation.state.cancel_requested = operation.requested()?;
    if operation.state.cancel_requested {
        operation.state.status = Status::Cancelled;
    } else if let Err(error) = result {
        operation.state.status = Status::Failed;
        operation.state.error = Some(error.to_string());
    } else if operation.state.phase != Phase::Verifying {
        operation.state.status = Status::Failed;
        operation.state.error = Some("Install finished before verification".into());
    } else {
        operation.state.status = Status::Succeeded;
        operation.state.phase = Phase::Done;
    }
    save(&operation.dir, &operation.state)?;
    Ok(operation.state)
}

fn prune_history(dir: &Path, current: Uuid) -> io::Result<()> {
    let mut history = Vec::new();
    for entry in dir.read_dir()? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "json")
            && let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            && id != current
        {
            history.push(id);
        }
    }
    history.sort_unstable_by(|a, b| b.cmp(a));
    for id in history.into_iter().skip(9) {
        match fs::remove_file(dir.join(format!("{id}.log"))) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            result => result?,
        }
        fs::remove_file(dir.join(format!("{id}.json")))?;
    }
    Ok(())
}
