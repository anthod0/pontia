//! Detached ownership of a JSON-lines subprocess. Native protocol state belongs to its client.
use super::{
    control_socket_path,
    rpc::{MAX_FRAME_BYTES, PROTOCOL_VERSION, RpcPeer, RpcRequest},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap, io::Read, path::PathBuf, process::Stdio, sync::Arc, time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::{ChildStdin, Command},
    sync::{Mutex, watch},
};

#[derive(Deserialize, Serialize)]
pub struct Bootstrap {
    pub pontia_home: PathBuf,
    pub session_id: String,
    pub runtime_id: String,
    pub client_session_key: Option<String>,
    pub cwd: PathBuf,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

#[derive(Default)]
struct State {
    binding: Option<String>,
    context: Value,
}

pub async fn run_from_stdin() -> Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let config: Bootstrap = serde_json::from_str(&input)?;
    run(config).await
}

pub async fn run(config: Bootstrap) -> Result<()> {
    let mut child = Command::new(&config.command)
        .args(&config.args)
        .envs(&config.environment)
        .env("PONTIA_SESSION_ID", &config.session_id)
        .current_dir(&config.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let input = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
    let output = child.stdout.take().expect("piped stdout");
    let state = Mutex::new(State {
        binding: config.client_session_key.clone(),
        ..Default::default()
    });
    let (attached, mut connection) = watch::channel::<Option<Arc<RpcPeer>>>(None);
    let mut reader = tokio::spawn(async move {
        let mut reader = BufReader::new(output);
        let generation = uuid::Uuid::now_v7();
        let mut sequence = 0u64;
        let mut frame = Vec::new();
        while reader.read_until(b'\n', &mut frame).await? != 0 {
            sequence += 1;
            let peer = connection.borrow_and_update().clone();
            if let Some(peer) = peer {
                // A line is forwarded once on its original connection, never across reconnect.
                let frame_id = format!("{generation}:{sequence}");
                let count = frame.len().div_ceil(64 * 1024);
                for (index, chunk) in frame.chunks(64 * 1024).enumerate() {
                    if peer
                        .request(
                            "relay.frame",
                            json!({"frame_id":frame_id, "chunk":STANDARD.encode(chunk), "last":index + 1 == count}),
                            MAX_FRAME_BYTES,
                            false,
                        )
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
            frame.clear();
        }
        Ok::<_, Error>(())
    });
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut reader_joined = false;
    let result = async {
        tokio::select! {
            result = connect(&config, &state, &input, &attached) => result,
            _ = terminate.recv() => Ok(()),
            result = &mut reader => {
                reader_joined = true;
                result.map_err(|error| Error::ControlUnknown(error.to_string()))?
            },
            result = child.wait() => {
                result?;
                // Drain the final native response before closing its controller connection.
                let drained = tokio::time::timeout(Duration::from_secs(5), &mut reader).await
                    .map_err(|_| Error::ControlUnknown("Relay output drain timed out".into()))?;
                reader_joined = true;
                drained.map_err(|error| Error::ControlUnknown(error.to_string()))?
            },
        }
    }
    .await;
    if let Some(peer) = attached.send_replace(None) {
        peer.close();
    }
    if let Some(pid) = child.id()
        && let Some(process) = ProcessFingerprint::capture(pid)
    {
        let _ = GenericRuntimeManager.terminate_process(&process);
    }
    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        child.kill().await?;
    }
    if !reader_joined {
        reader.abort();
        let _ = reader.await;
    }
    result
}

async fn connect(
    config: &Bootstrap,
    state: &Mutex<State>,
    input: &Arc<Mutex<ChildStdin>>,
    attached: &watch::Sender<Option<Arc<RpcPeer>>>,
) -> Result<()> {
    let path = control_socket_path(&config.pontia_home)?;
    let mut delay = Duration::from_millis(250);
    loop {
        if let Ok(stream) = UnixStream::connect(&path).await {
            let (peer, mut requests) = RpcPeer::new(stream);
            let binding = state.lock().await.binding.clone();
            let attach = peer.request(
                "runtime.attach",
                json!({
                    "version":PROTOCOL_VERSION, "session_id":config.session_id,
                    "runtime_id":config.runtime_id, "client_session_key":binding,
                }),
                MAX_FRAME_BYTES,
                false,
            );
            tokio::pin!(attach);
            let result = loop {
                tokio::select! {
                    result = &mut attach => break result,
                    request = requests.recv() => {
                        let Some(request) = request else { break Err(Error::ControlUnknown("Relay attachment disconnected".into())); };
                        match handle(&peer, request, config, state, input).await {
                            Ok(true) => return Ok(()),
                            Ok(false) => {},
                            Err(error) => break Err(error),
                        }
                    }
                }
            };
            let accepted = match result {
                Ok(response) if response.get("error").is_some() => {
                    return Err(Error::StateConflict("Relay identity was rejected".into()));
                }
                Ok(response) => {
                    if response["result"]["session_id"] != config.session_id
                        || response["result"]["runtime_id"] != config.runtime_id
                    {
                        return Err(Error::StateConflict(
                            "Relay attachment identity changed".into(),
                        ));
                    }
                    true
                }
                Err(_) => false,
            };
            if accepted {
                delay = Duration::from_millis(250);
                attached.send_replace(Some(peer.clone()));
                let ready = peer
                    .request("relay.ready", json!({}), MAX_FRAME_BYTES, false)
                    .await;
                if ready.is_ok_and(|response| response.get("error").is_none()) {
                    while let Some(request) = requests.recv().await {
                        match handle(&peer, request, config, state, input).await {
                            Ok(true) => return Ok(()),
                            Ok(false) => {}
                            Err(_) => break,
                        }
                    }
                }
            }
            attached.send_replace(None);
            peer.close();
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(5));
    }
}

async fn handle(
    peer: &Arc<RpcPeer>,
    request: RpcRequest,
    config: &Bootstrap,
    state: &Mutex<State>,
    input: &Arc<Mutex<ChildStdin>>,
) -> Result<bool> {
    match request.method.as_str() {
        "relay.status" => {
            let state = state.lock().await;
            peer.reply(
                request.id,
                json!({"context":state.context,"cwd":config.cwd}),
            )
            .await?;
        }
        "relay.state" => {
            let mut state = state.lock().await;
            if let Some(binding) = request.params["client_session_key"].as_str() {
                if state
                    .binding
                    .as_deref()
                    .is_some_and(|existing| existing != binding)
                {
                    return Err(Error::StateConflict("Relay native identity changed".into()));
                }
                state.binding = Some(binding.into());
            }
            state.context = request.params["context"].clone();
            peer.reply(request.id, json!({})).await?;
        }
        "relay.write" => {
            let bytes = STANDARD
                .decode(
                    request.params["chunk"]
                        .as_str()
                        .ok_or_else(|| Error::Domain("Relay write requires bytes".into()))?,
                )
                .map_err(|_| Error::Domain("Invalid relay bytes".into()))?;
            let input = input.clone();
            let peer = peer.clone();
            // A blocked native stdin must not prevent status, state updates, output, or exit.
            tokio::spawn(async move {
                let result = async {
                    let mut input = input.lock().await;
                    input.write_all(&bytes).await?;
                    input.flush().await
                }
                .await;
                if result.is_ok() {
                    let _ = peer.reply(request.id, json!({})).await;
                } else {
                    peer.close();
                }
            });
        }
        "relay.exit" => {
            peer.reply(request.id, json!({})).await?;
            return Ok(true);
        }
        _ => {
            peer.reply_error(request.id, -32601, "Unknown relay method")
                .await?
        }
    }
    Ok(false)
}
