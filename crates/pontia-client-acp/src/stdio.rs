use std::{future::Future, path::Path, process::Stdio, time::Duration};

use agent_client_protocol::ByteStreams;
use pontia_core::{Error, Result};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{AcpLaunch, peer::protocol_error};

pub(crate) async fn run<T, F>(
    launch: &AcpLaunch,
    cwd: &Path,
    timeout_message: &'static str,
    connect: impl FnOnce(ByteStreams<Compat<ChildStdin>, Compat<ChildStdout>>) -> F,
) -> Result<T>
where
    F: Future<Output = agent_client_protocol::Result<Result<T>>>,
{
    let mut child = Command::new(&launch.command)
        .args(&launch.args)
        .envs(&launch.env)
        .env_remove("PONTIA_SESSION_ID")
        .env_remove("PONTIA_RUNTIME_INSTANCE_ID")
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let writer = child.stdin.take().expect("piped stdin");
    let reader = child.stdout.take().expect("piped stdout");
    let operation = connect(ByteStreams::new(writer.compat_write(), reader.compat()));
    let result = tokio::time::timeout(Duration::from_secs(30), operation).await;
    // Closing stdin lets agents shut down any runtime processes they own.
    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(status) => {
            status?;
        }
        Err(_) => child.kill().await?,
    }
    result
        .map_err(|_| Error::CapabilityUnavailable(timeout_message.into()))?
        .map_err(protocol_error)?
}
