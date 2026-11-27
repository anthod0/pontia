use pontia_agent_control::{
    AgentControl, ClientControlChannel, ClientControlOperation, DeliveryOperation,
    EventIngestService, InputPayload, SendMode, SendOutcome,
    client_contract::{
        DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior, TerminateBehavior,
        TmuxRuntimeBehavior, TurnLifecycleBehavior, test_registration,
    },
    clients::ClientRegistry,
};
use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use serde_json::json;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Channel {
    events: EventIngestService,
    inputs: Mutex<Vec<String>>,
    exits: AtomicUsize,
    exit_attempts: AtomicUsize,
    busy: AtomicBool,
}
impl ClientControlChannel for Channel {
    fn available(&self) -> bool {
        true
    }
    fn invalidate(&self) {}
    fn interrupt(&self) -> DeliveryOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        panic!("exit must be conditional")
    }
    fn shutdown_if_idle(&self) -> ClientControlOperation<'_, bool> {
        Box::pin(async {
            self.exit_attempts.fetch_add(1, Ordering::SeqCst);
            if self.busy.load(Ordering::SeqCst) {
                return Ok(false);
            }
            self.exits.fetch_add(1, Ordering::SeqCst);
            self.events
                .report_client_fact(
                    "session",
                    Some("runtime"),
                    EventType::SessionExited,
                    json!({"reason":"quit"}),
                )
                .await?;
            Ok(true)
        })
    }
    fn ping(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        panic!("unexpected replay")
    }
    fn start_turn<'a>(&'a self, payload: &'a InputPayload) -> DeliveryOperation<'a> {
        Box::pin(async move {
            self.inputs.lock().unwrap().push(payload.text.clone());
            Ok(())
        })
    }
}

fn clients() -> ClientRegistry {
    static SPEC: OnceLock<pontia_agent_control::client_contract::AgentClientSpec> = OnceLock::new();
    let mut registration = test_registration();
    registration.spec = SPEC.get_or_init(|| {
        let mut spec = registration.spec.clone();
        spec.adapter.dispatch = DispatchBehavior::Connected;
        spec.adapter.terminate = TerminateBehavior::Connected;
        spec.adapter.runtime = RuntimeBehavior::Tmux(TmuxRuntimeBehavior { hook_log: None });
        spec.adapter.runtime_binding = RuntimeBindingBehavior::Named {
            runtime_kind: "test",
        };
        spec.adapter.turn_lifecycle = TurnLifecycleBehavior::ClientManagedInteractive;
        spec
    });
    let mut registry = ClientRegistry::default();
    registry.register(registration);
    registry
}

async fn open(root: &std::path::Path) -> AgentControl {
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        root.join("test.db").display()
    ))
    .await
    .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    AgentControl::builder(pool, root.into())
        .clients(clients())
        .build()
}
async fn connect(state: &AgentControl) -> Arc<Channel> {
    let channel = Arc::new(Channel {
        events: state.event_ingest_service(),
        inputs: Mutex::default(),
        exits: AtomicUsize::new(0),
        exit_attempts: AtomicUsize::new(0),
        busy: AtomicBool::new(false),
    });
    state
        .client_control()
        .attach("generic", "session", "runtime", "native", channel.clone())
        .await
        .unwrap();
    channel
}
async fn fixture() -> (tempfile::TempDir, AgentControl, Arc<Channel>) {
    let root = tempfile::tempdir().unwrap();
    let state = open(root.path()).await;
    sqlx::raw_sql("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle');
        INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','generic_control','running','2026-01-01T00:00:00Z');
        INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key) VALUES ('binding','session','generic','/unused','native');")
        .execute(&state.db()).await.unwrap();
    let channel = connect(&state).await;
    (root, state, channel)
}
async fn fact(
    state: &AgentControl,
    kind: EventType,
    turn: Option<&str>,
    payload: serde_json::Value,
) {
    state
        .event_ingest_service()
        .ingest_reported_event(ReportedEvent::new(
            pontia_core::ids::new_event_id().to_string(),
            "session".into(),
            turn.map(str::to_owned),
            EventSource::AgentClient,
            "generic".into(),
            kind,
            payload,
        ))
        .await
        .unwrap();
}
async fn wait_for(state: &AgentControl, expected: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if state
                .queries()
                .session_snapshot("session")
                .await
                .unwrap()
                .unwrap()
                .state
                == expected
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn exits_after_finishing_a_turn() {
    let (_root, state, channel) = fixture().await;
    fact(&state, EventType::TurnStarted, Some("first"), json!({})).await;
    state
        .session_commands()
        .exit_when_done("session")
        .await
        .unwrap();
    fact(&state, EventType::TurnCompleted, Some("first"), json!({})).await;
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
    assert_eq!(
        state
            .queries()
            .session_snapshot("session")
            .await
            .unwrap()
            .unwrap()
            .turns[0]
            .state,
        "completed"
    );
}

#[tokio::test]
async fn after_idle_input_is_delivered_and_consumed_before_exit() {
    let (_root, state, channel) = fixture().await;
    fact(&state, EventType::TurnStarted, Some("first"), json!({})).await;
    let SendOutcome::Queued { message_id } = state
        .inbox_commands()
        .send("session", "follow up".into(), SendMode::Queue)
        .await
        .unwrap()
    else {
        panic!("busy session input must be queued");
    };
    state
        .session_commands()
        .exit_when_done("session")
        .await
        .unwrap();
    fact(&state, EventType::TurnCompleted, Some("first"), json!({})).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if state
                .inbox_commands()
                .get_message("session", &message_id)
                .await
                .unwrap()
                .is_none()
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(*channel.inputs.lock().unwrap(), ["follow up"]);
    assert_eq!(channel.exits.load(Ordering::SeqCst), 0);
    assert!(
        !state
            .queries()
            .session_snapshot("session")
            .await
            .unwrap()
            .unwrap()
            .has_unsettled_input
    );
    fact(&state, EventType::TurnStarted, Some("second"), json!({})).await;
    fact(&state, EventType::TurnCompleted, Some("second"), json!({})).await;
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn restart_recovers_an_armed_idle_session_without_a_new_fact() {
    let (root, state, channel) = fixture().await;
    state.inbox_commands().stop_scheduling().await;
    state
        .session_commands()
        .exit_when_done("session")
        .await
        .unwrap();
    drop(channel);
    state.client_control().close().await;
    state.db().close().await;
    drop(state);
    let restarted = open(root.path()).await;
    let channel = connect(&restarted).await;
    restarted.inbox_commands().resume_pending().await.unwrap();
    wait_for(&restarted, "exited").await;
    restarted.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unmarked_session_stays_idle_after_completion_and_recovery() {
    let (_root, state, channel) = fixture().await;
    fact(&state, EventType::TurnStarted, Some("first"), json!({})).await;
    fact(&state, EventType::TurnCompleted, Some("first"), json!({})).await;
    state.inbox_commands().resume_pending().await.unwrap();
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 0);
    wait_for(&state, "idle").await;
}

#[tokio::test]
async fn non_idle_states_and_active_turns_do_not_exit() {
    for session_state in [
        "created",
        "starting",
        "busy",
        "waiting_for_interaction",
        "interrupted",
        "error",
        "exited",
    ] {
        let (_root, state, channel) = fixture().await;
        sqlx::query("UPDATE sessions SET state=? WHERE session_id='session'")
            .bind(session_state)
            .execute(&state.db())
            .await
            .unwrap();
        state
            .session_commands()
            .exit_when_done("session")
            .await
            .unwrap();
        state.inbox_commands().stop_scheduling().await;
        assert_eq!(channel.exits.load(Ordering::SeqCst), 0, "{session_state}");
        wait_for(&state, session_state).await;
    }
    for turn_state in ["queued", "running"] {
        let (_root, state, channel) = fixture().await;
        sqlx::query("INSERT INTO turns(turn_id,session_id,state) VALUES ('turn','session',?)")
            .bind(turn_state)
            .execute(&state.db())
            .await
            .unwrap();
        state
            .session_commands()
            .exit_when_done("session")
            .await
            .unwrap();
        state.inbox_commands().stop_scheduling().await;
        assert_eq!(channel.exits.load(Ordering::SeqCst), 0, "{turn_state}");
    }
}

#[tokio::test]
async fn pending_input_blocks_exit_until_removed() {
    let (_root, state, channel) = fixture().await;
    sqlx::query("INSERT INTO inbox_messages(message_id,session_id,input_summary) VALUES ('message','session','task')")
        .execute(&state.db()).await.unwrap();
    assert!(
        !state
            .session_commands()
            .exit_if_idle("session")
            .await
            .unwrap()
    );
    assert_eq!(channel.exits.load(Ordering::SeqCst), 0);

    sqlx::query("DELETE FROM inbox_messages WHERE message_id='message'")
        .execute(&state.db())
        .await
        .unwrap();
    assert!(
        state
            .session_commands()
            .exit_if_idle("session")
            .await
            .unwrap()
    );
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn client_busy_refusal_waits_for_the_next_availability_event() {
    let (_root, state, channel) = fixture().await;
    channel.busy.store(true, Ordering::SeqCst);
    state
        .session_commands()
        .exit_when_done("session")
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while channel.exit_attempts.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(channel.exits.load(Ordering::SeqCst), 0);
    wait_for(&state, "idle").await;
    channel.busy.store(false, Ordering::SeqCst);
    state.event_ingest_service().control_available("session");
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelling_the_last_pending_message_triggers_exit() {
    let (_root, state, channel) = fixture().await;
    // A persisted, armed Session awaiting delivery after its control channel reconnects.
    sqlx::raw_sql("UPDATE sessions SET exit_when_done=1;
        INSERT INTO inbox_messages(message_id,session_id,input_summary) VALUES ('pending','session','task');")
        .execute(&state.db()).await.unwrap();
    state
        .inbox_commands()
        .cancel_message("session", "pending")
        .await
        .unwrap();
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert!(channel.inputs.lock().unwrap().is_empty());
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unblocking_an_idle_session_triggers_exit() {
    let (_root, state, channel) = fixture().await;
    fact(
        &state,
        EventType::SessionReady,
        None,
        json!({"runtime_id":"runtime"}),
    )
    .await;
    fact(
        &state,
        EventType::InteractionBlocked,
        None,
        json!({"interaction_id":"question"}),
    )
    .await;
    wait_for(&state, "waiting_for_interaction").await;
    state
        .session_commands()
        .exit_when_done("session")
        .await
        .unwrap();
    fact(
        &state,
        EventType::InteractionUnblocked,
        None,
        json!({"interaction_id":"question"}),
    )
    .await;
    wait_for(&state, "exited").await;
    state.inbox_commands().stop_scheduling().await;
    assert_eq!(channel.exits.load(Ordering::SeqCst), 1);
}
