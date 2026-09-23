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

async fn register(pi: &PiRpcPeer, root: &std::path::Path) -> (String, String) {
    let result = pi
        .call(
            "runtime.register",
            json!({"version":PROTOCOL_VERSION,"binding":{
                "client_type":"pi", "client_session_key":"native", "client_cwd":root,
                "tmux":{"socket_path":"/unused/pi-test-tmux","pane_id":"%1"}
            }}),
        )
        .await
        .unwrap();
    (
        result["session"]["session_id"].as_str().unwrap().into(),
        result["runtime"]["runtime_instance_id"]
            .as_str()
            .unwrap()
            .into(),
    )
}

fn fact(session: &str, runtime: &str, kind: &str, data: serde_json::Value) -> serde_json::Value {
    json!({"runtime_instance_id":runtime,"event":{"session_id":session,"type":kind,"data":data}})
}

#[tokio::test]
async fn branch_replay_lost_acknowledgement_is_unknown_and_not_replayed_after_attach() {
    let (state, root) = state().await;
    let (pi, mut requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    let control = state.pi_control();
    let replay = control.replay(&session, &runtime, "msg_replay");
    let disconnect = async {
        let request = requests.recv().await.unwrap();
        assert_eq!(request.method, "branch.replay");
        assert_eq!(request.params, json!({"inbox_message_id": "msg_replay"}));
        pi.close();
    };
    let (result, ()) = tokio::join!(replay, disconnect);
    assert!(matches!(result, Err(pontia_core::Error::ControlUnknown(_))));
    let (replacement, mut requests) = client(&state);
    replacement
        .call(
            "runtime.attach",
            json!({
                "version": PROTOCOL_VERSION, "session_id": session,
                "runtime_instance_id": runtime, "client_session_key": "native",
            }),
        )
        .await
        .unwrap();
    let ping = control.ping(&session, &runtime);
    let respond = async {
        let request = requests.recv().await.unwrap();
        assert_eq!(request.method, "ping");
        replacement
            .reply(request.id, json!({"pong": true}))
            .await
            .unwrap();
    };
    let (result, ()) = tokio::join!(ping, respond);
    result.unwrap();
    assert!(requests.try_recv().is_err());
    replacement.close();
}

#[tokio::test]
async fn reports_use_shared_fact_processing_and_acknowledge_exit_before_closing() {
    let (state, root) = state().await;
    let (pi, mut requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    let ready = pi
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "session.ready",
                json!({
                    "runtime_instance_id":runtime, "client_session_key":"native"
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(ready["accepted"], true);
    // A control request can be outstanding while the client reports a fact.
    let ping = {
        let control = state.pi_control();
        let session = session.clone();
        let runtime = runtime.clone();
        tokio::spawn(async move { control.ping(&session, &runtime).await })
    };
    let request = requests.recv().await.unwrap();
    let started = pi
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "turn.started",
                json!({
                    "runtime_instance_id":runtime, "input_summary":"界".repeat(40_000)
                }),
            ),
        )
        .await
        .unwrap();
    pi.reply(request.id, json!({"pong":true})).await.unwrap();
    ping.await.unwrap().unwrap();
    let turn = started["turn_id"].as_str().unwrap();
    let summary: String = sqlx::query_scalar("SELECT json_extract(payload, '$.input.summary') FROM events WHERE event_type='turn.started' AND session_id=?")
        .bind(&session).fetch_one(&state.db()).await.unwrap();
    assert_eq!(summary, "界".repeat(200));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.db())
        .await
        .unwrap();
    let refreshed = pi
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "session.message_updated",
                json!({"reason":"append"}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(refreshed["accepted"], true);
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(after, count, "message refresh remains volatile");
    let mut completed = fact(
        &session,
        &runtime,
        "turn.completed",
        json!({"terminal_leaf_id":null}),
    );
    completed["event"]["turn_id"] = json!(turn);
    assert_eq!(
        pi.call("event.report", completed).await.unwrap()["accepted"],
        true
    );
    let exited = pi
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "session.exited",
                json!({"runtime_instance_id":runtime,"reason":"quit"}),
            ),
        )
        .await
        .unwrap();
    assert_eq!(exited["accepted"], true);
    tokio::time::timeout(Duration::from_secs(1), pi.closed())
        .await
        .unwrap();
    let state_value: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
        .bind(&session)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(state_value, "exited");
}

#[tokio::test]
async fn reporting_rejects_unregistered_cross_session_and_stale_identities_and_invalid_envelopes() {
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    assert!(
        pi.call(
            "event.report",
            fact("unknown", "r", "session.message_updated", json!({}))
        )
        .await
        .is_err()
    );
    assert!(
        pi.call(
            "turn.startFailure",
            json!({"session_id":"unknown","runtime_instance_id":"r","reason":"transport_failed"})
        )
        .await
        .is_err()
    );
    let (session, runtime) = register(&pi, root.path()).await;
    let valid = fact(
        &session,
        &runtime,
        "session.message_updated",
        json!({"reason":"append"}),
    );
    for field in [
        "timeline_boundary",
        "event_id",
        "source",
        "client_type",
        "time",
    ] {
        let mut request = valid.clone();
        request["event"][field] = json!("client supplied");
        assert!(pi.call("event.report", request).await.is_err());
    }
    let mut removed = valid.clone();
    removed["event"]["type"] = json!("turn.timeline_item");
    assert!(pi.call("event.report", removed).await.is_err());
    let mut foreign = valid.clone();
    foreign["event"]["session_id"] = json!("other");
    assert!(pi.call("event.report", foreign).await.is_err());
    let mut stale = valid.clone();
    stale["runtime_instance_id"] = json!("old");
    assert!(pi.call("event.report", stale).await.is_err());
    sqlx::query("UPDATE runtime_bindings SET runtime_instance_id='replacement' WHERE session_id=?")
        .bind(&session)
        .execute(&state.db())
        .await
        .unwrap();
    assert!(pi.call("event.report", valid).await.is_err());
    assert!(
        pi.call(
            "turn.startFailure",
            json!({"session_id":session,"runtime_instance_id":runtime,"client_session_key":"native","reason":"transport_failed"})
        )
        .await
        .is_err()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.error'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
    pi.close();
}

#[tokio::test]
async fn failure_reports_are_idempotent_after_a_lost_started_acknowledgement() {
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    let started = pi
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "turn.started",
                json!({"runtime_instance_id":runtime}),
            ),
        )
        .await
        .unwrap();
    // Only the failure notification is retried, over an independent connection.
    pi.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.pi_control().available(&session).await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (pi, _requests) = client(&state);
    let failure = json!({"session_id":session,"runtime_instance_id":runtime,"client_session_key":"native","reason":"transport_failed"});
    assert_eq!(
        pi.call("turn.startFailure", failure.clone()).await.unwrap()["accepted"],
        true
    );
    pi.close();
    let (retry, _requests) = client(&state);
    assert!(!state.pi_control().available(&session).await.unwrap());
    assert_eq!(
        retry.call("turn.startFailure", failure).await.unwrap()["accepted"],
        true
    );
    retry.close();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.error'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(count, 1);
    let turn_state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(started["turn_id"].as_str().unwrap())
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(turn_state, "abandoned");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('turn.started','turn.failed')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(count, 1);
    pi.close();
}

#[tokio::test]
async fn rpc_queries_read_workspace_session_and_pinned_or_latest_profiles() {
    use pontia_application::{AgentProfileService, UpsertExecutionProfileRequest};
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    assert_eq!(
        pi.call("workspaces.list", json!({})).await.unwrap(),
        json!({"workspaces": []})
    );
    let (session, _) = register(&pi, root.path()).await;
    let workspaces = pi.call("workspaces.list", json!({})).await.unwrap();
    assert_eq!(
        workspaces["workspaces"][0]["canonical_path"],
        root.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(workspaces["workspaces"][0]["state"], "active");
    let service = AgentProfileService::new(state.db());
    for version in ["1", "2"] {
        let request: UpsertExecutionProfileRequest = serde_json::from_value(json!({
            "profile_id": "reviewer", "version": version, "name": "Reviewer", "agent_kind": "executor",
            "supported_client_types": ["pi"], "system_prompt_template": format!("Prompt {version}")
        })).unwrap();
        if version == "1" {
            service.create_profile(request).await.unwrap();
        } else {
            service
                .create_profile_version("reviewer", request)
                .await
                .unwrap();
        }
    }
    sqlx::query("UPDATE sessions SET execution_profile_id='reviewer', execution_profile_version='1' WHERE session_id=?")
        .bind(&session).execute(&state.db()).await.unwrap();
    let result = pi
        .call("session.get", json!({"session_id": session}))
        .await
        .unwrap();
    assert_eq!(result["session"]["execution_profile_id"], "reviewer");
    assert_eq!(result["session"]["execution_profile_version"], "1");
    assert_eq!(
        pi.call(
            "profile.get",
            json!({"profile_id":"reviewer", "version":"1"})
        )
        .await
        .unwrap()["agent_profile"]["system_prompt_template"],
        "Prompt 1"
    );
    assert_eq!(
        pi.call("profile.get", json!({"profile_id":"reviewer"}))
            .await
            .unwrap()["agent_profile"]["system_prompt_template"],
        "Prompt 2"
    );
    for (method, params) in [
        ("session.get", json!({"session_id":"missing"})),
        (
            "profile.get",
            json!({"profile_id":"reviewer", "version":"missing"}),
        ),
        ("profile.get", json!({"profile_id":"missing"})),
        ("session.get", json!({})),
        ("workspaces.list", json!({"unexpected": true})),
    ] {
        assert!(pi.call(method, params).await.is_err());
    }
    pi.close();
}
