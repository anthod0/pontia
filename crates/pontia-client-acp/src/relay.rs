//! Detached ACP Agent process and its in-memory reports.
mod reports;
use base64::{Engine, engine::general_purpose::STANDARD};
use pontia_agent_control::ipc::{
    control_socket_path,
    rpc::{MAX_FRAME_BYTES, PROTOCOL_VERSION, RpcPeer, RpcRequest},
};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint};
pub(crate) use reports::{Observation, Report};
use reports::{Reports, drain, send_frame};
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
    setup_status: Option<&'static str>,
    reports: Reports,
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
        .env("PONTIA_RUNTIME_INSTANCE_ID", &config.runtime_id)
        .current_dir(&config.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let input = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
    let output = child.stdout.take().expect("piped stdout");
    let state = Arc::new(Mutex::new(State {
        binding: config.client_session_key.clone(),
        ..Default::default()
    }));
    let (attached, mut connection) = watch::channel::<Option<Arc<RpcPeer>>>(None);
    let reader_state = state.clone();
    let mut reader = tokio::spawn(async move {
        let mut reader = BufReader::new(output);
        let mut frame = Vec::new();
        while reader.read_until(b'\n', &mut frame).await? != 0 {
            let native: Value = serde_json::from_slice(&frame)?;
            let (forward, barrier) = {
                let mut state = reader_state.lock().await;
                let State {
                    context, reports, ..
                } = &mut *state;
                let forward = reports.observe(context, &native);
                (forward, reports.barrier())
            };
            let peer = connection.borrow_and_update().clone();
            if forward && let Some(peer) = peer {
                if native["method"] != "$/cancel_request"
                    && let Some(mut barrier) = barrier
                {
                    tokio::select! {
                        _ = barrier.changed() => {},
                        _ = connection.changed() => { frame.clear(); continue; }
                    }
                }
                let id = pontia_core::ids::new_event_id().to_string();
                let _ = send_frame(&peer, &id, &native).await;
            }
            frame.clear();
        }
        while !reader_state.lock().await.reports.is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
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
    state: &Arc<Mutex<State>>,
    input: &Arc<Mutex<ChildStdin>>,
    attached: &watch::Sender<Option<Arc<RpcPeer>>>,
) -> Result<()> {
    let path = control_socket_path(&config.pontia_home)?;
    let mut delay = Duration::from_millis(250);
    loop {
        if let Ok(stream) = UnixStream::connect(&path).await {
            let (peer, mut requests) = RpcPeer::new(stream);
            let mut answer = Vec::new();
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
                        let Some(request) = request else { break attach.await };
                        match handle(&peer, request, config, state, input, &mut answer).await {
                            Ok(true) => return Ok(()),
                            Ok(false) => {},
                            Err(error) => break Err(error),
                        }
                    }
                }
            };
            let accepted = match result {
                Ok(response)
                    if response.pointer("/error/code").and_then(Value::as_i64) == Some(-32603) =>
                {
                    false
                }
                Ok(response) if response.get("error").is_some() => {
                    eprintln!("Discarding queued ACP reports after permanent attachment rejection");
                    return Err(Error::StateConflict(
                        response["error"]["message"]
                            .as_str()
                            .unwrap_or("Relay attachment was rejected")
                            .into(),
                    ));
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
                    state.lock().await.reports.reattach();
                    let sending = drain(&peer, state, input);
                    tokio::pin!(sending);
                    loop {
                        tokio::select! {
                            _ = &mut sending => break,
                            request = requests.recv() => {
                                let Some(request) = request else { break };
                                match handle(&peer, request, config,state,input, &mut answer).await {
                                    Ok(true) => return Ok(()), Ok(false) => {}, Err(_) => break,
                                }
                            }
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
    state: &Arc<Mutex<State>>,
    input: &Arc<Mutex<ChildStdin>>,
    answer: &mut Vec<u8>,
) -> Result<bool> {
    match request.method.as_str() {
        "relay.status" => {
            let state = state.lock().await;
            peer.reply(
                request.id,
                json!({"context":state.context,"setup_status":state.setup_status,"cwd":config.cwd}),
            )
            .await?;
        }
        "relay.setup" => {
            let mut locked = state.lock().await;
            if locked.setup_status.is_some() {
                drop(locked);
                peer.reply_error(
                    request.id,
                    -32009,
                    "ACP setup was already handed to the relay",
                )
                .await?;
                return Ok(false);
            }
            let setup = serde_json::from_value(request.params["setup"].clone())?;
            locked.context = request.params["context"].clone();
            locked.setup_status = Some("pending");
            let receipt = locked.reports.push(setup);
            drop(locked);
            let peer = peer.clone();
            let state = state.clone();
            tokio::spawn(async move {
                let confirmed = matches!(receipt.await, Ok(Ok(())));
                state.lock().await.setup_status =
                    Some(if confirmed { "confirmed" } else { "rejected" });
                if confirmed {
                    let _ = peer.reply(request.id, json!({})).await;
                } else {
                    let _ = peer
                        .reply_error(request.id, -32009, "ACP setup rejected")
                        .await;
                }
            });
        }
        "relay.bind" => {
            let key = request.params["client_session_key"]
                .as_str()
                .ok_or_else(|| Error::Domain("Missing binding".into()))?;
            state.lock().await.binding = Some(key.into());
            peer.reply(request.id, json!({})).await?;
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
        "relay.write" | "relay.steer_prompt" => {
            let mut bytes = STANDARD
                .decode(
                    request.params["chunk"]
                        .as_str()
                        .ok_or_else(|| Error::Domain("Relay write requires bytes".into()))?,
                )
                .map_err(|_| Error::Domain("Invalid relay bytes".into()))?;
            if let Some(id) = request.params.get("close_request_id") {
                state.lock().await.reports.close_request_id = Some(id.clone());
            }
            if request.method == "relay.steer_prompt" {
                answer.append(&mut bytes);
                if request.params["last"] != true {
                    peer.reply(request.id, json!({"buffered":true})).await?;
                    return Ok(false);
                }
                bytes = std::mem::take(answer);
                let frame: Value = serde_json::from_slice(&bytes)?;
                let mut locked = state.lock().await;
                let State {
                    context, reports, ..
                } = &mut *locked;
                if !reports.steer_prompt(context, &request.params["turn"], &frame)? {
                    drop(locked);
                    peer.reply(request.id, json!({"written":false})).await?;
                    // Release the local SDK request without writing a new prompt to the idle Agent.
                    let result = json!({"jsonrpc":"2.0","id":frame["id"],"error":{"code":-32600,"message":"Prompt already finished"}});
                    let peer = peer.clone();
                    tokio::spawn(async move {
                        let _ = send_frame(
                            &peer,
                            &pontia_core::ids::new_event_id().to_string(),
                            &result,
                        )
                        .await;
                    });
                    return Ok(false);
                }
            }
            let auth = request.params.get("permission");
            let permission = if let Some(auth) = auth {
                let key = auth["key"]
                    .as_str()
                    .ok_or_else(|| Error::Domain("Missing permission identity".into()))?;
                if !state.lock().await.reports.permission(&auth["id"], key) {
                    answer.clear();
                    peer.reply(request.id, json!({"written":false})).await?;
                    return Ok(false);
                }
                answer.extend_from_slice(&bytes);
                if auth["last"] != true {
                    peer.reply(request.id, json!({"buffered":true})).await?;
                    return Ok(false);
                }
                let frame: Value = serde_json::from_slice(answer)?;
                if frame["id"] != auth["id"] {
                    return Err(Error::Domain("ACP answer identity changed".into()));
                }
                let selected = frame
                    .pointer("/result/outcome/outcome")
                    .and_then(Value::as_str)
                    == Some("selected");
                Some((
                    auth["id"].clone(),
                    key.to_owned(),
                    std::mem::take(answer),
                    selected,
                ))
            } else {
                None
            };
            let input = input.clone();
            let state = state.clone();
            let peer = peer.clone();
            // A blocked native stdin must not prevent status, state updates, output, or exit.
            tokio::spawn(async move {
                let result = async {
                    let mut input = input.lock().await;
                    if let Some((id, key, bytes, selected)) = permission {
                        let Some(report) = state.lock().await.reports.begin_write(&id, &key) else {
                            return Ok(false);
                        };
                        input.write_all(&bytes).await?;
                        input.flush().await?;
                        state.lock().await.reports.flushed(id, report, selected);
                    } else {
                        input.write_all(&bytes).await?;
                        input.flush().await?;
                    }
                    Ok::<_, std::io::Error>(true)
                }
                .await;
                if let Ok(written) = result {
                    let _ = peer.reply(request.id, json!({"written":written})).await;
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
