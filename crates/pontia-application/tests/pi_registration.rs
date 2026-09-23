use pontia_application::{
    AppState,
    pi_ipc::{PiIpcListener, serve_connection},
};
use pontia_runtime::pi_control::{PROTOCOL_VERSION, PiRpcPeer};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::net::UnixStream;

async fn state() -> (AppState, tempfile::TempDir) {
    let root = tempfile::Builder::new().prefix("pr-").tempdir().unwrap();
    let db = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&db).await.unwrap();
    (AppState::builder(db, root.path().into()).build(), root)
}

fn client(
    state: &AppState,
) -> (
    Arc<PiRpcPeer>,
    tokio::sync::mpsc::Receiver<pontia_runtime::pi_control::RpcRequest>,
) {
    let (server, client) = UnixStream::pair().unwrap();
    tokio::spawn(serve_connection(state.clone(), server));
    PiRpcPeer::new(client)
}

#[tokio::test]
async fn registration_establishes_identity_and_reconnect_never_resumes_or_replaces_a_runtime() {
    let (state, root) = state().await;
    let (pi, mut requests) = client(&state);
    assert_eq!(
        pi.call("session.context", json!({"client_session_key":"native"}))
            .await
            .unwrap(),
        json!({"session_context":null})
    );
    let registration = json!({"version":PROTOCOL_VERSION,"binding":{
        "client_type":"pi","client_session_key":"native","client_cwd":root.path(),
        "tmux":{"socket_path":"/unused/pi-test-tmux","pane_id":"%1"}
    }});
    let registered = pi
        .call("runtime.register", registration.clone())
        .await
        .unwrap();
    let session = registered["session"]["session_id"].as_str().unwrap();
    let runtime = registered["runtime"]["runtime_instance_id"]
        .as_str()
        .unwrap();
    assert!(state.pi_control().available(session).await.unwrap());
    assert_eq!(registered["session"]["state"], "starting");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.ready'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
    let ping = {
        let control = state.pi_control();
        let session = session.to_owned();
        let runtime = runtime.to_owned();
        tokio::spawn(async move { control.ping(&session, &runtime).await })
    };
    // Query in the reverse direction while the daemon's ping is awaiting a reply.
    let context = pi
        .call("session.context", json!({"client_session_key":"native"}))
        .await
        .unwrap();
    assert_eq!(context["session_context"]["session_id"], session);
    let request = requests.recv().await.unwrap();
    assert_eq!(request.method, "ping");
    pi.reply(request.id, json!({"pong":true})).await.unwrap();
    ping.await.unwrap().unwrap();
    assert!(pi.call("runtime.register", registration).await.is_err());
    let attach = json!({"version":PROTOCOL_VERSION,"session_id":session,"runtime_instance_id":runtime,"client_session_key":"native"});
    let (other, _requests) = client(&state);
    assert!(
        other.call("runtime.attach", attach.clone()).await.is_err(),
        "a second live connection cannot take ownership"
    );
    pi.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.pi_control().available(session).await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        other.call("runtime.attach", attach.clone()).await.unwrap()["runtime_instance_id"],
        runtime
    );
    other.close();
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id=?")
        .bind(session)
        .execute(&state.db())
        .await
        .unwrap();
    let (stale, _requests) = client(&state);
    assert!(stale.call("runtime.attach", attach).await.is_err());
    let current:(String,String)=sqlx::query_as("SELECT s.state,r.runtime_instance_id FROM sessions s JOIN runtime_bindings r ON r.session_id=s.session_id WHERE s.session_id=?").bind(session).fetch_one(&state.db()).await.unwrap();
    assert_eq!(current, ("exited".into(), runtime.into()));
    stale.close();
    state.pi_control().close().await;
}

#[tokio::test]
async fn rejects_old_protocol_and_invalid_registration_before_mutating_business_state() {
    let (state, _root) = state().await;
    let (pi, _requests) = client(&state);
    for (method, params) in [
        ("hello", json!({"session_id":"s","runtime_instance_id":"r"})),
        (
            "runtime.register",
            json!({"version":2,"binding":{"client_type":"pi","client_session_key":"n"}}),
        ),
        (
            "runtime.register",
            json!({"version":PROTOCOL_VERSION,"binding":{"client_type":"codex","client_session_key":"n"}}),
        ),
        (
            "runtime.attach",
            json!({"version":PROTOCOL_VERSION,"session_id":"s","runtime_instance_id":"old","client_session_key":"unknown"}),
        ),
    ] {
        assert!(pi.call(method, params).await.is_err());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 0);
    pi.close();
}

#[tokio::test]
async fn listener_preserves_occupied_paths_and_recovers_a_stale_socket() {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let root = tempfile::Builder::new().prefix("pl-").tempdir().unwrap();
    let directory = root.path().join("state/pi");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("rpc.sock");
    std::fs::write(&path, "owned file").unwrap();
    assert!(PiIpcListener::bind(root.path()).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned file");
    std::fs::remove_file(&path).unwrap();
    drop(UnixListener::bind(&path).unwrap());
    let listener = PiIpcListener::bind(root.path()).await.unwrap();
    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(PiIpcListener::bind(root.path()).await.is_err());
    assert!(path.exists());
    drop(listener);
    assert!(!path.exists());
}
