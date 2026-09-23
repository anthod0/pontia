//! Pi registration transport. Binding and lifecycle decisions remain in application services.
use crate::{
    AgentBindingService, AppState, RuntimeBindingUpsertRequest, RuntimeBindingUpsertService,
};
use pontia_core::{Error, Result};
use pontia_runtime::pi_control::{PROTOCOL_VERSION, PiRpcPeer, RpcRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::watch,
    task::JoinSet,
};

pub struct PiIpcListener {
    listener: UnixListener,
    path: PathBuf,
    inode: u64,
    owner: u32,
    _lock: std::fs::File,
}

impl PiIpcListener {
    pub async fn bind(pontia_home: &Path) -> Result<Self> {
        let directory = pontia_home.join("state/pi");
        std::fs::create_dir_all(&directory)?;
        if !std::fs::symlink_metadata(&directory)?.file_type().is_dir() {
            return Err(Error::Domain(
                "Pi socket directory must be a real directory".into(),
            ));
        }
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(directory.join("rpc.lock"))?;
        lock.try_lock().map_err(|error| {
            Error::StateConflict(format!(
                "Pi listener is already running or cannot be locked: {error}"
            ))
        })?;
        let path = directory.join("rpc.sock");
        if path.as_os_str().as_encoded_bytes().len() > 103 {
            return Err(Error::Domain(
                "Pontia home is too long for the Pi Unix socket (maximum 103 bytes)".into(),
            ));
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(Error::Domain(
                    "Pi socket path is occupied by a non-socket file".into(),
                ));
            }
            match UnixStream::connect(&path).await {
                Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&path)?
                }
                _ => return Err(Error::StateConflict("Pi socket is already in use".into())),
            }
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let metadata = std::fs::metadata(&path)?;
        Ok(Self {
            listener,
            path,
            inode: metadata.ino(),
            owner: metadata.uid(),
            _lock: lock,
        })
    }

    pub async fn run(self, state: AppState, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut connections = JoinSet::new();
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    if stream.peer_cred()?.uid() != self.owner { continue; }
                    let state = state.clone();
                    connections.spawn(async move { serve_connection(state, stream).await; });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        Ok(())
    }
}

impl Drop for PiIpcListener {
    fn drop(&mut self) {
        if std::fs::symlink_metadata(&self.path)
            .is_ok_and(|metadata| metadata.ino() == self.inode && metadata.file_type().is_socket())
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct ClosePeer(Arc<PiRpcPeer>);
impl Drop for ClosePeer {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub async fn serve_connection(state: AppState, stream: UnixStream) {
    let (peer, mut requests) = PiRpcPeer::new(stream);
    let _close = ClosePeer(peer.clone());
    let mut registered = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let request = tokio::select! {
            biased;
            _ = peer.closed() => break,
            _ = tokio::time::sleep_until(deadline), if !registered => break,
            request = requests.recv() => request,
        };
        let Some(request) = request else { break };
        if request.id.is_none() {
            continue;
        } // Registration requires an acknowledged response.
        if !matches!(
            request.method.as_str(),
            "session.context" | "runtime.register" | "runtime.attach"
        ) {
            if peer
                .reply_error(request.id, -32601, "Unknown Pi registration method")
                .await
                .is_err()
            {
                break;
            }
            continue;
        }
        let result = dispatch(&state, &peer, &request, &mut registered).await;
        let response = match result {
            Ok(value) => peer.reply(request.id, value).await,
            Err(error) => {
                let code = match &error {
                    Error::NotFound(_) => -32004,
                    Error::StateConflict(_) | Error::CapabilityUnavailable(_) => -32009,
                    Error::Domain(_) => -32009,
                    Error::Serialization(_) => -32602,
                    _ => -32603,
                };
                let message = match &error {
                    Error::Domain(message)
                    | Error::StateConflict(message)
                    | Error::NotFound(message)
                    | Error::CapabilityUnavailable(message) => message.clone(),
                    _ => error.to_string(),
                };
                peer.reply_error(request.id, code, &message).await
            }
        };
        if response.is_err() {
            break;
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    version: u32,
    binding: RuntimeBindingUpsertRequest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attach {
    version: u32,
    session_id: String,
    runtime_instance_id: String,
    client_session_key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextQuery {
    client_session_key: String,
}

async fn dispatch(
    state: &AppState,
    peer: &Arc<PiRpcPeer>,
    request: &RpcRequest,
    registered: &mut bool,
) -> Result<Value> {
    if request.method == "session.context" {
        let query: ContextQuery = serde_json::from_value(request.params.clone())?;
        if query.client_session_key.trim().is_empty() {
            return Err(Error::Domain("client_session_key is required".into()));
        }
        let context = AgentBindingService::new(state.db())
            .session_context_for_client_session("pi", &query.client_session_key)
            .await?;
        return Ok(json!({"session_context":context}));
    }
    if *registered {
        return Err(Error::StateConflict(
            "Connection is already registered".into(),
        ));
    }
    let (identity, result) = match request.method.as_str() {
        "runtime.register" => {
            let registration: Registration = serde_json::from_value(request.params.clone())?;
            version(registration.version)?;
            if registration.binding.client_type != "pi" {
                return Err(Error::Domain(
                    "Pi registration requires client_type pi".into(),
                ));
            }
            let client_session_key = registration.binding.client_session_key.clone();
            let result = RuntimeBindingUpsertService::new(state.db(), state.pontia_home().into())
                .upsert(registration.binding)
                .await?;
            let session_id = result["session"]["session_id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Registration returned no session identity".into()))?
                .to_owned();
            let runtime_instance_id = result["runtime"]["runtime_instance_id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Registration returned no runtime identity".into()))?
                .to_owned();
            (
                Attach {
                    version: registration.version,
                    session_id,
                    runtime_instance_id,
                    client_session_key,
                },
                result,
            )
        }
        "runtime.attach" => {
            let identity: Attach = serde_json::from_value(request.params.clone())?;
            version(identity.version)?;
            let result = json!({"session_id":identity.session_id,"runtime_instance_id":identity.runtime_instance_id});
            (identity, result)
        }
        _ => return Err(Error::Domain("Unknown Pi registration method".into())),
    };
    state
        .pi_control()
        .attach(
            &identity.session_id,
            &identity.runtime_instance_id,
            &identity.client_session_key,
            peer.clone(),
        )
        .await?;
    *registered = true;
    crate::InboxCommandService::new(state.event_ingest_service())
        .notify_available(&identity.session_id);
    Ok(result)
}

impl crate::PiControlChannel for PiRpcPeer {
    fn available(&self) -> bool {
        !self.is_closed()
    }
    fn invalidate(&self) {
        self.close();
    }
    fn ping(&self) -> crate::PiControlOperation<'_> {
        Box::pin(async {
            if self.call("ping", json!({})).await? != json!({"pong":true}) {
                return Err(Error::ControlUnknown("Invalid Pi ping response".into()));
            }
            Ok(())
        })
    }
    fn submit<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
    ) -> crate::PiControlOperation<'a> {
        Box::pin(async move {
            if self
                .call(
                    "submit",
                    json!({"input":input,"inbox_message_id":inbox_message_id}),
                )
                .await?
                != json!({"accepted":true})
            {
                return Err(Error::ControlUnknown("Invalid Pi submit response".into()));
            }
            Ok(())
        })
    }
}

fn version(version: u32) -> Result<()> {
    if version != PROTOCOL_VERSION {
        return Err(Error::Domain("Unsupported Pi RPC protocol version".into()));
    }
    Ok(())
}
