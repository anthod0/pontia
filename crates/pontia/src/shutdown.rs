use std::{
    io,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    process::Stdio,
};

pub use pontia_agent_control::runtime::shutdown::Operation;
use pontia_agent_control::runtime::shutdown::{ShutdownRequest, ShutdownUpdate};
use pontia_config::AppConfig;
use pontia_runtime::launch_lock::{LaunchLock, receiver_directory};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio_stream::StreamExt;
use uuid::Uuid;

use pontia::{
    client::ClientApi,
    lifecycle::{HealthProbe, RunState, ServiceManager, local_health_addr},
    runtime_io::HttpHealthProbe,
};

pub async fn run(
    config: &AppConfig,
    operation: Operation,
    force: bool,
    stop: bool,
    manager: &impl ServiceManager,
    listing: impl FnOnce(Vec<String>),
) -> Result<Option<LaunchLock>, String> {
    let receiver = if stop {
        let id = Uuid::now_v7();
        let path = receiver_directory(id);
        let directory = tempfile::Builder::new()
            .prefix(path.file_name().unwrap())
            .rand_bytes(0)
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir_in(path.parent().unwrap())
            .map_err(|e| e.to_string())?;
        let socket =
            UnixDatagram::bind(directory.path().join("lock")).map_err(|e| e.to_string())?;
        Some((id, directory, socket))
    } else {
        None
    };
    let request = ShutdownRequest {
        pontia_home: config.pontia_home.clone(),
        operation,
        force,
        stop,
        lock_receiver: receiver.as_ref().map(|r| r.0),
    };
    let addr = local_health_addr(config.bind_addr);
    let response = if HttpHealthProbe.is_healthy(addr)? {
        ClientApi::new(config)?.shutdown(&request).await?
    } else {
        None
    };
    if let Some(response) = response {
        let body = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(io::Error::other));
        read_updates(tokio_util::io::StreamReader::new(body), stop, listing).await?;
    } else {
        let definition = manager.definition_path(&crate::user_home()?);
        if definition.try_exists().map_err(|error| error.to_string())?
            && matches!(
                manager.status()?.run_state,
                RunState::Running | RunState::Starting
            )
        {
            return Err(format!(
                "The running Pontia service is unavailable at http://{addr}"
            ));
        }
        let mut child = tokio::process::Command::new(crate::sibling_pontiad()?)
            .arg("shutdown")
            .env("PONTIA_HOME", &config.pontia_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| e.to_string())?;
        let input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&input)
            .await
            .map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().unwrap();
        let (updates, output) = tokio::join!(
            read_updates(stdout, stop, listing),
            child.wait_with_output()
        );
        let output = output.map_err(|e| e.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(if stderr.trim().is_empty() {
                format!("pontiad shutdown exited with {}", output.status)
            } else {
                stderr.trim_end().into()
            });
        }
        updates?;
    }
    receiver
        .map(|(_, _directory, socket)| LaunchLock::receive(&socket))
        .transpose()
        .map_err(|e| e.to_string())
}

async fn read_updates(
    reader: impl AsyncRead + Unpin,
    stop: bool,
    listing: impl FnOnce(Vec<String>),
) -> Result<(), String> {
    let mut lines = BufReader::new(reader).lines();
    let first = next_update(&mut lines).await?;
    let ShutdownUpdate::Listing(items) = first else {
        return Err("Missing shutdown listing".into());
    };
    listing(items);
    if stop {
        let ShutdownUpdate::Stopped(warnings) = next_update(&mut lines).await? else {
            return Err("Missing shutdown result".into());
        };
        for warning in warnings {
            eprintln!("{warning}");
        }
    }
    Ok(())
}

async fn next_update<R: AsyncRead + Unpin>(
    lines: &mut tokio::io::Lines<BufReader<R>>,
) -> Result<ShutdownUpdate, String> {
    let line = lines
        .next_line()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Incomplete shutdown response")?;
    match serde_json::from_str(&line).map_err(|e| e.to_string())? {
        ShutdownUpdate::Failed(message) => Err(message),
        update => Ok(update),
    }
}
