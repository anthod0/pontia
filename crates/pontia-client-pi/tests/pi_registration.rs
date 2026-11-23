mod support;
use pontia_agent_control::AgentControl;
use pontia_agent_control::ipc::ControlListener;
use pontia_agent_control::ipc::rpc::PROTOCOL_VERSION;
use pontia_client_pi::ipc::serve_connection;
use pontia_client_pi::rpc::PiRpcPeer;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::net::UnixStream;

async fn state() -> (AgentControl, tempfile::TempDir) {
    let root = tempfile::Builder::new().prefix("pr-").tempdir().unwrap();
    let db = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&db).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &db,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        db.clone(),
    )
    .save("pi", "default", true)
    .await
    .unwrap();
    (
        AgentControl::builder(db, root.path().into())
            .clients(support::clients())
            .build(),
        root,
    )
}

fn client(
    state: &AgentControl,
) -> (
    Arc<PiRpcPeer>,
    tokio::sync::mpsc::Receiver<pontia_agent_control::ipc::rpc::RpcRequest>,
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
        "client_type":"pi","client_session_key":"native","client_cwd":root.path()
    }});
    let registered = pi
        .call("runtime.register", registration.clone())
        .await
        .unwrap();
    let session = registered["session"]["session_id"].as_str().unwrap();
    let runtime = registered["runtime"]["runtime_id"].as_str().unwrap();
    sqlx::query("DROP TABLE agent_profiles")
        .execute(&state.db())
        .await
        .unwrap();
    let snapshot = pi
        .call("session.get", json!({"session_id":session}))
        .await
        .unwrap();
    assert_eq!(
        snapshot["session_instructions"]["system_prompt"],
        pontia_agent_control::standalone_session_instructions()
    );
    assert!(state.client_control().available(session).await.unwrap());
    assert_eq!(registered["session"]["state"], "starting");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.ready'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
    let ping = {
        let control = state.client_control();
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
    let attach = json!({"version":PROTOCOL_VERSION,"session_id":session,"runtime_id":runtime,"client_session_key":"native"});
    let (other, _requests) = client(&state);
    assert!(
        other.call("runtime.attach", attach.clone()).await.is_err(),
        "a second live connection cannot take ownership"
    );
    pi.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.client_control().available(session).await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        other.call("runtime.attach", attach.clone()).await.unwrap()["runtime_id"],
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
    let current:(String,String)=sqlx::query_as("SELECT s.state,r.runtime_id FROM sessions s JOIN session_runtimes r ON r.session_id=s.session_id WHERE s.session_id=?").bind(session).fetch_one(&state.db()).await.unwrap();
    assert_eq!(current, ("exited".into(), runtime.into()));
    stale.close();
    state.client_control().close().await;
}

#[tokio::test]
async fn disabling_pi_blocks_new_native_registration_but_preserves_existing_registration_and_facts()
{
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    pontia_agent_control::agent_clients::AgentClientService::new(state.db(), state.clients())
        .save("pi", "default", false)
        .await
        .unwrap();
    let (new_pi, _requests) = client(&state);
    assert!(new_pi.call("runtime.register", json!({"version":PROTOCOL_VERSION,"binding":{
        "client_type":"pi", "client_session_key":"another-native", "client_cwd":root.path()
    }})).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    pi.call(
        "event.report",
        fact(
            &session,
            &runtime,
            "session.ready",
            json!({"client_session_key":"native"}),
        ),
    )
    .await
    .unwrap();
    pi.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.client_control().available(&session).await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (replacement, _requests) = client(&state);
    let rebound = replacement.call("runtime.register", json!({"version":PROTOCOL_VERSION, "binding":{
        "client_type":"pi", "session_id":session, "runtime_id":runtime, "client_session_key":"native", "client_cwd":root.path()
    }})).await.unwrap();
    assert_eq!(rebound["runtime"]["runtime_id"], runtime);
    assert_eq!(rebound["session"]["session_id"], session);
    assert!(state.client_control().available(&session).await.unwrap());
    replacement.close();
    new_pi.close();
    state.client_control().close().await;
}

#[tokio::test]
async fn rejects_old_protocol_and_invalid_registration_before_mutating_business_state() {
    let (state, _root) = state().await;
    for (method, params) in [
        ("hello", json!({"session_id":"s","runtime_id":"r"})),
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
            json!({"version":PROTOCOL_VERSION,"session_id":"s","runtime_id":"old","client_session_key":"unknown"}),
        ),
    ] {
        let (pi, _requests) = client(&state);
        assert!(pi.call(method, params).await.is_err());
        pi.close();
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn incompatible_handshake_marks_the_runtime_until_a_supported_attachment() {
    use pontia_agent_control::ipc::{
        protocol::UNSUPPORTED_VERSION,
        rpc::{MAX_FRAME_BYTES, RpcPeer},
    };
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    pi.close();
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.client_control().available(&session).await.unwrap() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    for (method, params) in [
        (
            "runtime.attach",
            json!({"session_id":session,"runtime_id":runtime,"client_session_key":"native","version":9}),
        ),
        (
            "runtime.register",
            json!({"binding":{"client_type":"pi","client_session_key":"native"}}),
        ),
    ] {
        let (server, client) = UnixStream::pair().unwrap();
        tokio::spawn(pontia_agent_control::ipc::serve_connection(
            state.clone(),
            server,
        ));
        let (peer, _) = RpcPeer::new(client);
        if method == "runtime.register" {
            peer.request(
                "session.context",
                json!({"client_session_key":"native"}),
                MAX_FRAME_BYTES,
                false,
            )
            .await
            .unwrap();
        }
        let response = peer
            .request(method, params, MAX_FRAME_BYTES, false)
            .await
            .unwrap();
        assert_eq!(response["error"]["code"], UNSUPPORTED_VERSION);
        let view = state
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(view.control_connected, Some(false));
        assert_eq!(
            view.control_disconnected_reason,
            Some("unsupported_protocol_version")
        );
        peer.close();
        let (valid, _) = self::client(&state);
        valid.call("runtime.attach", json!({"session_id":session,"runtime_id":runtime,"client_session_key":"native","version":PROTOCOL_VERSION})).await.unwrap();
        assert_eq!(
            state
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .control_disconnected_reason,
            None
        );
        valid.close();
        tokio::time::timeout(Duration::from_secs(1), async {
            while state.client_control().available(&session).await.unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn listener_preserves_occupied_paths_and_recovers_a_stale_socket() {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let root = tempfile::Builder::new().prefix("pl-").tempdir().unwrap();
    let path = pontia_agent_control::ipc::control_socket_path(root.path()).unwrap();
    let directory = path.parent().unwrap();
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(&path, "owned file").unwrap();
    assert!(ControlListener::bind(root.path()).await.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned file");
    std::fs::remove_file(&path).unwrap();
    drop(UnixListener::bind(&path).unwrap());
    let listener = ControlListener::bind(root.path()).await.unwrap();
    assert_eq!(
        std::fs::metadata(directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(ControlListener::bind(root.path()).await.is_err());
    assert!(path.exists());
    drop(listener);
    assert!(!path.exists());
}

async fn register(pi: &PiRpcPeer, root: &std::path::Path) -> (String, String) {
    let result = pi
        .call(
            "runtime.register",
            json!({"version":PROTOCOL_VERSION,"binding":{
                "client_type":"pi", "client_session_key":"native", "client_cwd":root
            }}),
        )
        .await
        .unwrap();
    (
        result["session"]["session_id"].as_str().unwrap().into(),
        result["runtime"]["runtime_id"].as_str().unwrap().into(),
    )
}

fn fact(session: &str, runtime: &str, kind: &str, data: serde_json::Value) -> serde_json::Value {
    json!({"runtime_id":runtime,"event":{
        "session_id":session,
        "type":kind,
        "report_key":pontia_core::ids::new_event_id().to_string(),
        "data":data
    }})
}

#[tokio::test]
async fn compaction_control_and_facts_share_identity_without_changing_session_state() {
    let (state, root) = state().await;
    let (pi, mut requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    pi.call(
        "event.report",
        fact(
            &session,
            &runtime,
            "session.ready",
            json!({"client_session_key":"native"}),
        ),
    )
    .await
    .unwrap();
    let capabilities = state
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(
        capabilities.manual_compaction
            && capabilities.compaction_events
            && capabilities.compaction_history
    );
    assert!(
        state
            .client_control()
            .compact(&session, "stale-runtime", "rejected")
            .await
            .is_err()
    );
    assert!(requests.try_recv().is_err());
    let command = state.session_commands();
    let receipt = command.compact_session(&session);
    let native = async {
        let request = requests.recv().await.unwrap();
        assert_eq!(request.method, "compact");
        let operation = request.params["operation_id"].as_str().unwrap().to_owned();
        pi.reply(request.id, json!({"accepted":true}))
            .await
            .unwrap();
        operation
    };
    let (receipt, operation) = tokio::join!(receipt, native);
    assert_eq!(receipt.unwrap().operation_id, operation);
    for kind in ["session.compaction_started", "session.compaction_completed"] {
        let payload = json!({"operation_id":operation, "trigger":"manual"});
        let request = fact(&session, &runtime, kind, payload);
        let first = pi.call("event.report", request.clone()).await.unwrap();
        let duplicate = pi.call("event.report", request).await.unwrap();
        assert_eq!(first["event_id"], duplicate["event_id"]);
    }
    assert_eq!(
        state
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    pi.close();
    let capabilities = state
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap()
        .capabilities;
    assert!(!capabilities.manual_compaction);
    assert!(capabilities.compaction_events && capabilities.compaction_history);
}

#[tokio::test]
async fn branch_replay_lost_acknowledgement_is_unknown_and_not_replayed_after_attach() {
    let (state, root) = state().await;
    let (pi, mut requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    let control = state.client_control();
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
                "runtime_id": runtime, "client_session_key": "native",
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
                    "runtime_id":runtime, "client_session_key":"native"
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(ready["accepted"], true);
    // A control request can be outstanding while the client reports a fact.
    let ping = {
        let control = state.client_control();
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
                    "runtime_id":runtime, "input_summary":"界".repeat(40_000)
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
                json!({"runtime_id":runtime,"reason":"quit"}),
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
            fact(
                "unknown",
                "r",
                "session.context_usage_updated",
                json!({"context_usage":{"used_tokens":1}}),
            )
        )
        .await
        .is_err()
    );
    let (session, runtime) = register(&pi, root.path()).await;
    let valid = fact(
        &session,
        &runtime,
        "session.context_usage_updated",
        json!({"context_usage":{"used_tokens":1}}),
    );
    let mut missing_report_key = valid.clone();
    missing_report_key["event"]
        .as_object_mut()
        .unwrap()
        .remove("report_key");
    assert!(pi.call("event.report", missing_report_key).await.is_err());
    let mut empty_report_key = fact(
        &session,
        &runtime,
        "turn.started",
        json!({"runtime_id":runtime}),
    );
    empty_report_key["event"]["report_key"] = json!("  ");
    assert!(pi.call("event.report", empty_report_key).await.is_err());
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
    stale["runtime_id"] = json!("old");
    assert!(pi.call("event.report", stale).await.is_err());
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE session_id=?")
        .bind(&session)
        .execute(&state.db())
        .await
        .unwrap();
    assert!(pi.call("event.report", valid).await.is_err());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.error'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(count, 0);
    pi.close();
}

#[tokio::test]
async fn repeated_started_reports_return_the_first_canonical_identity() {
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    let report = fact(
        &session,
        &runtime,
        "turn.started",
        json!({"runtime_id":runtime,"input_summary":"work"}),
    );

    let started = pi.call("event.report", report.clone()).await.unwrap();
    let duplicate = pi.call("event.report", report.clone()).await.unwrap();
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["event_id"], started["event_id"]);
    assert_eq!(duplicate["turn_id"], started["turn_id"]);

    let mut changed = report.clone();
    changed["event"]["data"]["input_summary"] = json!("changed");
    assert!(pi.call("event.report", changed).await.is_err());
    let mut supplied_turn = report;
    supplied_turn["event"]["turn_id"] = started["turn_id"].clone();
    assert!(pi.call("event.report", supplied_turn).await.is_err());
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='turn.started'",
    )
    .bind(&session)
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(count, 1);
    pi.close();
}

#[tokio::test]
async fn replacement_connection_can_only_acknowledge_committed_facts_of_the_previous_runtime() {
    let (state, root) = state().await;
    let (old, _requests) = client(&state);
    let (session, original_runtime) = register(&old, root.path()).await;
    let mut request = fact(
        &session,
        &original_runtime,
        "session.ready",
        json!({"runtime_id":original_runtime,"client_session_key":"native"}),
    );
    let first = old.call("event.report", request.clone()).await.unwrap();
    let mut model = fact(
        &session,
        &original_runtime,
        "session.model_updated",
        json!({"runtime_id":original_runtime,"model":"example"}),
    );
    let model_ack = old.call("event.report", model.clone()).await.unwrap();
    old.close();
    sqlx::query("UPDATE session_runtimes SET runtime_id='replacement' WHERE runtime_id=?")
        .bind(&original_runtime)
        .execute(&state.db())
        .await
        .unwrap();
    let (new, _requests) = client(&state);
    new.call("runtime.attach", json!({"version":PROTOCOL_VERSION,"session_id":session,"runtime_id":"replacement","client_session_key":"native"})).await.unwrap();
    request["runtime_id"] = json!("replacement");
    let mut broadcasts = state.agent_events().subscribe();
    let count = || {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE session_id=?")
            .bind(&session)
    };
    let before = count().fetch_one(&state.db()).await.unwrap();
    let duplicate = new.call("event.report", request.clone()).await.unwrap();
    assert_eq!(duplicate["accepted"], true);
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["event_id"], first["event_id"]);
    assert_eq!(duplicate["state_version"], first["state_version"]);
    model["runtime_id"] = json!("replacement");
    let model_replay = new.call("event.report", model).await.unwrap();
    assert_eq!(model_replay["event_id"], model_ack["event_id"]);
    assert_eq!(model_replay["duplicate"], true);
    let mut changed = request.clone();
    changed["event"]["data"]["client_session_key"] = json!("changed");
    let error = new.call("event.report", changed).await.unwrap_err();
    assert!(error.to_string().contains("-32009"), "{error}");
    request["event"]["report_key"] = json!("uncommitted");
    assert!(new.call("event.report", request).await.is_err());
    let old_usage = fact(
        &session,
        "replacement",
        "session.context_usage_updated",
        json!({"runtime_id":original_runtime,"context_usage":{"used_tokens":2}}),
    );
    assert!(new.call("event.report", old_usage).await.is_err());
    assert_eq!(before, count().fetch_one(&state.db()).await.unwrap());
    assert!(matches!(
        broadcasts.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    new.close();
}

#[tokio::test]
async fn rpc_queries_read_workspace_and_session_instructions() {
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    assert_eq!(
        pi.call("workspaces.list", json!({})).await.unwrap()["workspaces"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let (session, _) = register(&pi, root.path()).await;
    let workspaces = pi.call("workspaces.list", json!({})).await.unwrap();
    assert_eq!(
        workspaces["workspaces"][0]["canonical_path"],
        root.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(workspaces["workspaces"][0]["state"], "active");
    let result = pi
        .call("session.get", json!({"session_id": session}))
        .await
        .unwrap();
    assert_eq!(
        result["session_instructions"]["system_prompt"],
        pontia_agent_control::standalone_session_instructions()
    );
    for (method, params) in [
        ("session.get", json!({"session_id":"missing"})),
        ("session.get", json!({})),
        ("workspaces.list", json!({"unexpected": true})),
    ] {
        assert!(pi.call(method, params).await.is_err());
    }
    pi.close();
}

#[tokio::test]
async fn restarting_a_stable_runtime_rejects_old_connection_facts_and_receipts() {
    use pontia_agent_control::{PontiaEvent, PontiaEventSource, PontiaEventType};
    let (state, root) = state().await;
    let (old, mut requests) = client(&state);
    let (session, runtime) = register(&old, root.path()).await;
    old.call(
        "event.report",
        fact(
            &session,
            &runtime,
            "session.ready",
            json!({"client_session_key":"native"}),
        ),
    )
    .await
    .unwrap();
    let created: String =
        sqlx::query_scalar("SELECT created_at FROM session_runtimes WHERE runtime_id=?")
            .bind(&runtime)
            .fetch_one(&state.db())
            .await
            .unwrap();
    let pending = {
        let control = state.client_control();
        let (session, runtime) = (session.clone(), runtime.clone());
        tokio::spawn(async move { control.submit(&session, &runtime, "one input", None).await })
    };
    let receipt = requests.recv().await.unwrap();
    assert_eq!(receipt.method, "submit");
    old.call(
        "event.report",
        fact(
            &session,
            &runtime,
            "session.exited",
            json!({"reason":"quit"}),
        ),
    )
    .await
    .unwrap();
    state
        .event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            &session,
            None,
            PontiaEventSource::ExternalApi,
            "pi",
            PontiaEventType::SessionResuming,
            json!({"runtime_id":runtime}),
        ))
        .await
        .unwrap();
    let (current, mut requests) = client(&state);
    current.call("runtime.register", json!({"version":PROTOCOL_VERSION,"binding":{"client_type":"pi","client_cwd":root.path(),"runtime_id":runtime,"client_session_key":"native"}})).await.unwrap();
    current
        .call(
            "event.report",
            fact(
                &session,
                &runtime,
                "session.ready",
                json!({"client_session_key":"native"}),
            ),
        )
        .await
        .unwrap();
    assert!(
        old.call(
            "event.report",
            fact(
                &session,
                &runtime,
                "session.exited",
                json!({"reason":"late"})
            )
        )
        .await
        .is_err()
    );
    let _ = old.reply(receipt.id, json!({"accepted":true})).await;
    assert!(matches!(
        pending.await.unwrap(),
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    assert!(
        requests.try_recv().is_err(),
        "input with an unknown receipt is not replayed"
    );
    let record: (String,String,String) = sqlx::query_as("SELECT s.state,r.state,r.created_at FROM sessions s JOIN session_runtimes r USING(session_id) WHERE r.runtime_id=?").bind(&runtime).fetch_one(&state.db()).await.unwrap();
    assert_eq!(record, ("idle".into(), "running".into(), created));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&state.db())
            .await
            .unwrap(),
        1
    );
    current.close();
}

#[tokio::test]
async fn a_different_process_instance_cannot_attach_or_replace_the_registered_runtime() {
    let (state, root) = state().await;
    let (pi, _requests) = client(&state);
    let (session, runtime) = register(&pi, root.path()).await;
    pi.call(
        "event.report",
        fact(
            &session,
            &runtime,
            "session.ready",
            json!({"client_session_key":"native"}),
        ),
    )
    .await
    .unwrap();
    pi.close();
    // Represent a recorded previous instance of this PID; the IPC peer is the current instance.
    let mut previous = pontia_runtime::ProcessFingerprint::capture(std::process::id()).unwrap();
    previous.start_time_seconds -= 1;
    let previous = serde_json::to_string(&previous).unwrap();
    sqlx::query("UPDATE session_runtimes SET process_fingerprint=? WHERE runtime_id=?")
        .bind(&previous)
        .bind(&runtime)
        .execute(&state.db())
        .await
        .unwrap();
    let (other, _requests) = client(&state);
    assert!(other.call("runtime.attach", json!({"version":PROTOCOL_VERSION,"session_id":session,"runtime_id":runtime,"client_session_key":"native"})).await.is_err());
    assert!(other.call("runtime.register", json!({"version":PROTOCOL_VERSION,"binding":{
        "client_type":"pi","client_session_key":"native","client_cwd":root.path(),"runtime_id":runtime
    }})).await.is_err());
    let saved: (String, String) = sqlx::query_as("SELECT s.state,r.process_fingerprint FROM sessions s JOIN session_runtimes r USING(session_id) WHERE r.runtime_id=?")
        .bind(&runtime).fetch_one(&state.db()).await.unwrap();
    assert_eq!(saved, ("idle".into(), previous));
    other.close();
    state.client_control().close().await;
}

#[tokio::test]
async fn runtime_registration_cannot_register_or_reactivate_a_workspace() {
    let (state, root) = state().await;
    let cwd = root.path().join("unregistered");
    std::fs::create_dir(&cwd).unwrap();
    for workspace_state in [None, Some("archived"), Some("deleted")] {
        if let Some(workspace_state) = workspace_state {
            pontia_agent_control::workspaces::upsert_canonical_workspace(
                &state.db(),
                cwd.to_str().unwrap(),
                None,
            )
            .await
            .unwrap();
            sqlx::query("UPDATE workspaces SET state=? WHERE canonical_path=?")
                .bind(workspace_state)
                .bind(cwd.to_str().unwrap())
                .execute(&state.db())
                .await
                .unwrap();
        }
        let before: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT workspace_id,state,updated_at FROM workspaces ORDER BY workspace_id",
        )
        .fetch_all(&state.db())
        .await
        .unwrap();
        let (pi, _requests) = client(&state);
        assert!(pi.call("runtime.register", json!({"version":PROTOCOL_VERSION,"binding":{"client_type":"pi","client_session_key":"native","client_cwd":cwd}})).await.is_err());
        let after: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT workspace_id,state,updated_at FROM workspaces ORDER BY workspace_id",
        )
        .fetch_all(&state.db())
        .await
        .unwrap();
        assert_eq!(before, after);
        let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM sessions),(SELECT COUNT(*) FROM agent_bindings),(SELECT COUNT(*) FROM session_runtimes)").fetch_one(&state.db()).await.unwrap();
        assert_eq!(counts, (0, 0, 0));
        pi.close();
    }
}

#[tokio::test]
async fn precreated_session_binds_only_in_its_dedicated_directory_without_a_workspace() {
    use pontia_agent_control::{PontiaEvent, PontiaEventSource, PontiaEventType};
    let (state, root) = state().await;
    let directory = root.path().join("bot/current");
    let other = root.path().join("bot/other");
    std::fs::create_dir_all(&directory).unwrap();
    let directory = directory.canonicalize().unwrap();
    std::fs::create_dir(&other).unwrap();
    for (kind, payload) in [
        (
            PontiaEventType::SessionCreated,
            json!({"workspace":directory}),
        ),
        (
            PontiaEventType::SessionStarting,
            json!({"runtime_id":"dedicated-runtime"}),
        ),
    ] {
        state
            .event_ingest_service()
            .ingest_pontia_event(PontiaEvent::new(
                "dedicated-session",
                None,
                PontiaEventSource::ExternalApi,
                "pi",
                kind,
                payload,
            ))
            .await
            .unwrap();
    }
    let (pi, _requests) = client(&state);
    let request = |cwd: &std::path::Path| {
        json!({"version":PROTOCOL_VERSION,"binding":{
            "client_type":"pi", "client_session_key":"dedicated-session", "client_cwd":cwd,
            "runtime_id":"dedicated-runtime"
        }})
    };
    assert!(pi.call("runtime.register", request(&other)).await.is_err());
    let workspace = state.queries().list_workspaces().await.unwrap().remove(0);
    assert!(
        pi.call(
            "runtime.register",
            request(std::path::Path::new(&workspace.canonical_path))
        )
        .await
        .is_err()
    );
    let result = pi
        .call("runtime.register", request(&directory))
        .await
        .unwrap();
    assert_eq!(result["session"]["session_id"], "dedicated-session");
    assert_eq!(result["runtime"]["runtime_id"], "dedicated-runtime");
    let session = state
        .queries()
        .get_session("dedicated-session")
        .await
        .unwrap()
        .unwrap();
    assert!(session.workspace_id.is_none());
    assert_eq!(state.queries().list_workspaces().await.unwrap().len(), 1);
    pi.close();
    state.client_control().close().await;
}
