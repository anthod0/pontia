use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use pontia_agent_control::{
    AgentControl, EventIngestService, ReportedFact,
    agent_clients::AgentClientService,
    client_contract::{
        AgentInput, ApplicationLaunchSettings, ClientConfiguration, ClientLaunchRequest,
        ClientLauncher, GenericTestClient, InProcessClient, test_registration,
    },
    clients::ClientRegistry,
};
use pontia_bot::{
    conversation::{ConversationService, ConversationUpdate, Reply, ReplyRequest},
    identity::{BotIdentity, TOOL_TOKEN_ENV},
    messages::MessageStatus,
    records::BotConfiguration,
};
use pontia_core::{
    Error, Result,
    client_capabilities::AgentClientCapabilities,
    domain::{EventType, ReportedEvent},
};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartResult};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};

const MODEL: &str = "test/model";
const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    steer: true,
    ..pontia_agent_control::client_contract::TEST_SPEC.capabilities
};

#[derive(Default)]
struct Client {
    inputs: Mutex<Vec<AgentInput>>,
    handoffs: Mutex<Vec<String>>,
    blocked: Mutex<Option<String>>,
    tokens: Mutex<std::collections::HashMap<String, String>>,
}

impl ClientConfiguration for Client {
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, key: &str, _: &Value) -> (String, String) {
        (key.into(), key.into())
    }
    fn key(&self, _: &Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _: &str, overrides: &Value) -> Value {
        overrides.clone()
    }
    fn resolve(&self, _: &str, overrides: &Value) -> Result<Value> {
        Ok(overrides.clone())
    }
}

impl ClientLauncher for Client {
    fn supports_application_launch(&self) -> bool {
        true
    }

    fn application_command(
        &self,
        _: &Value,
        settings: &ApplicationLaunchSettings,
    ) -> Result<String> {
        if settings.model != MODEL {
            return Err(Error::CapabilityUnavailable("model unavailable".into()));
        }
        self.handoffs
            .lock()
            .unwrap()
            .push(std::fs::read_to_string(&settings.instructions_file)?);
        Ok("in-process".into())
    }

    fn validate_resume(&self, _: &str, _: Option<&Path>) -> Result<()> {
        Ok(())
    }

    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult> {
        self.tokens.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.environment[TOOL_TOKEN_ENV].clone(),
        );
        GenericRuntimeManager.start_in_process(request.root, request.runtime, CAPABILITIES, 0)
    }
}

impl pontia_agent_control::client_contract::ClientProfile for Client {
    fn validate_templates(&self, _: Option<&str>, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn bind(&self, profile: &pontia_agent_control::ExecutionProfileView) -> Result<Value> {
        Ok(json!({"system": profile.system_prompt_template}))
    }
}

impl InProcessClient for Client {
    fn capabilities(&self) -> AgentClientCapabilities {
        CAPABILITIES
    }
    fn ready(&self, session: &str, instance: &str) -> ReportedEvent {
        GenericTestClient::ready_event(session, instance)
    }

    fn submit(&self, input: AgentInput) -> Result<()> {
        let mut inputs = self.inputs.lock().unwrap();
        let first = !inputs.iter().any(|previous| previous.input == input.input);
        let failure = if self.blocked.lock().unwrap().as_deref() == Some(&input.session_id) {
            Err(Error::ControlUnknown("connection lost".into()))
        } else {
            match input.input.lines().last() {
                Some("reject once") if first => Err(Error::Domain("rejected".into())),
                Some("uncertain once") if first => {
                    Err(Error::ControlUnknown("acknowledgement lost".into()))
                }
                _ => Ok(()),
            }
        };
        inputs.push(input);
        failure
    }
}

fn bot_configuration(name: &str, model: &str) -> BotConfiguration {
    BotConfiguration {
        name: name.into(),
        description: None,
        client_type: "generic".into(),
        client_key: "default".into(),
        model: model.into(),
    }
}

async fn report(
    ingest: &EventIngestService,
    session: &str,
    runtime: &str,
    turn: &str,
    fact_type: EventType,
) {
    ingest
        .report_fact(ReportedFact {
            session_id: session.into(),
            turn_id: Some(turn.into()),
            report_key: None,
            fact_type,
            data: json!({"runtime_id": runtime}),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn bot_inputs_and_replies_use_the_owned_session_and_visible_chat() {
    let (_root, _pool, client, control, service) = fixture().await;

    let a = service
        .records()
        .create(bot_configuration("A", "missing/model"))
        .await
        .unwrap();
    let a = &a.bot_id;
    let failed = service.send(a, "historical failure").await.unwrap();
    assert_eq!(failed.status, MessageStatus::Failed);
    service
        .records()
        .update(a, bot_configuration("A", MODEL))
        .await
        .unwrap();
    let b = service
        .records()
        .create(bot_configuration("B", MODEL))
        .await
        .unwrap();
    let b = &b.bot_id;
    let wake = service.send(a, "wake A").await.unwrap();
    assert_eq!(wake.status, MessageStatus::Normal);
    service.send(b, "wake B").await.unwrap();
    let a_session = service
        .records()
        .get(a)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let b_session = service
        .records()
        .get(b)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    assert_ne!(a_session, b_session);
    assert!(
        control
            .queries()
            .list_workspaces()
            .await
            .unwrap()
            .is_empty()
    );

    let turns = control.queries().list_turns(&a_session).await.unwrap();
    assert_eq!(turns.len(), 1);
    let turn = &turns[0].turn_id;
    let snapshot = control
        .queries()
        .session_snapshot(&a_session)
        .await
        .unwrap()
        .unwrap();
    let runtime = &snapshot.runtimes[0].runtime_id;
    let ingest = control.event_ingest_service();
    report(&ingest, &a_session, runtime, turn, EventType::TurnStarted).await;
    for content in ["reject once", "uncertain once"] {
        let failed = service.send(a, content).await.unwrap();
        assert_eq!(failed.status, MessageStatus::Failed);
        assert!(service.retry(b, &failed.id).await.is_err());
        let retried = service.retry(a, &failed.id).await.unwrap();
        assert_eq!(retried.id, failed.id);
        assert_eq!(retried.status, MessageStatus::Normal);
    }
    assert_eq!(
        control
            .queries()
            .list_turns(&a_session)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        control
            .queries()
            .session_snapshot(&a_session)
            .await
            .unwrap()
            .unwrap()
            .inbox
            .is_empty()
    );

    report(&ingest, &a_session, runtime, turn, EventType::TurnFailed).await;
    let chat = service.chat(a).await.unwrap();
    assert_eq!(chat.notices.len(), 1);
    assert_eq!(chat.notices[0].kind, "turn.failed");
    assert_eq!(
        chat.messages
            .iter()
            .find(|message| message.id == wake.id)
            .unwrap()
            .status,
        MessageStatus::Normal
    );
    service.send(a, "wake after failure").await.unwrap();
    assert_eq!(
        control
            .queries()
            .list_turns(&a_session)
            .await
            .unwrap()
            .len(),
        2
    );

    let reply = |session: &str, text: &str, work: Option<&str>| ReplyRequest {
        session_id: session.into(),
        input: Reply {
            message: text.into(),
            working_on: work.map(str::to_owned),
        },
    };
    assert!(service.resolve(&a_session).await.is_ok());
    let token = client.tokens.lock().unwrap()[&a_session].clone();
    assert!(
        service
            .authenticate_tool(&a_session, Some(&token))
            .await
            .is_ok()
    );
    assert!(service.authenticate_tool(&a_session, None).await.is_err());
    assert!(
        service
            .authenticate_tool(&b_session, Some(&token))
            .await
            .is_err()
    );
    assert!(
        service
            .send_to_user(reply("foreign", "must not show", None))
            .await
            .is_err()
    );
    let mut updates = service.subscribe(a);
    service
        .send_to_user(reply(&a_session, "Complete\nreply", Some("Checking")))
        .await
        .unwrap();
    assert!(matches!(
        updates.recv().await.unwrap(),
        ConversationUpdate::Typing
    ));
    assert!(matches!(
        updates.recv().await.unwrap(),
        ConversationUpdate::Changed
    ));
    let chat = service.chat(a).await.unwrap();
    assert_eq!(chat.messages.last().unwrap().content, "Complete\nreply");
    assert_eq!(chat.bot.working_on.as_deref(), Some("Checking"));
    let restarted = ConversationService::new(control.clone());
    assert_eq!(
        restarted.chat(a).await.unwrap().bot.working_on.as_deref(),
        Some("Checking")
    );
    service
        .send_to_user(reply(&a_session, "Done", None))
        .await
        .unwrap();
    assert!(service.chat(a).await.unwrap().bot.working_on.is_none());
    assert_eq!(service.chat(b).await.unwrap().messages.len(), 1);

    {
        let handoffs = client.handoffs.lock().unwrap();
        assert_eq!(handoffs.len(), 2);
        assert!(handoffs[0].contains("historical failure"));
        assert!(!handoffs[0].contains("wake A"));
        assert!(!handoffs[1].contains("historical failure"));
    }
    let delivered = client.inputs.lock().unwrap().len();
    let recovered = ConversationService::new(control);
    let chat = recovered.chat(a).await.unwrap();
    assert_eq!(chat.messages[0].status, MessageStatus::Failed);
    assert_eq!(client.inputs.lock().unwrap().len(), delivered);
    let inputs = client.inputs.lock().unwrap();
    assert_eq!(inputs[0].input, "Source: User\n\nwake A");
    assert_eq!(inputs[0].session_id, a_session);
    assert_eq!(inputs[1].session_id, b_session);
    assert!(
        inputs
            .iter()
            .all(|input| !input.input.contains("historical failure"))
    );
}

async fn fixture() -> (
    tempfile::TempDir,
    sqlx::SqlitePool,
    Arc<Client>,
    AgentControl,
    ConversationService,
) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("state.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let client = Arc::new(Client::default());
    let mut registration = test_registration();
    const BOT_CLIENT: pontia_agent_control::client_contract::AgentClientSpec =
        pontia_agent_control::client_contract::AgentClientSpec {
            capabilities: CAPABILITIES,
            ..pontia_agent_control::client_contract::TEST_SPEC
        };
    registration.spec = &BOT_CLIENT;
    registration.configuration = Some(client.clone());
    registration.launcher = Some(client.clone());
    registration.in_process = Some(client.clone());
    registration.profile = Some(client.clone());
    let mut registry = ClientRegistry::default();
    registry.register(registration);
    AgentClientService::new(pool.clone(), registry.clone())
        .save("generic", "default", Some(true), None)
        .await
        .unwrap();
    let control = AgentControl::builder(pool.clone(), root.path().to_owned())
        .clients(registry)
        .build();
    let service = ConversationService::new(control.clone());

    (root, pool, client, control, service)
}

use pontia_bot::helpers::{CreateHelper, HelperCommand, HelperRequest, HelperService};

async fn bot(service: &ConversationService) -> (String, String) {
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL))
        .await
        .unwrap();
    service.send(&bot.bot_id, "start").await.unwrap();
    let session = service
        .records()
        .get(&bot.bot_id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    (bot.bot_id, session)
}

async fn helper(service: &HelperService, session: &str) -> String {
    let result = service
        .execute(HelperRequest {
            session_id: session.into(),
            command: HelperCommand::Create(CreateHelper {
                profile_id: None,
                profile_version: None,
                description: "assigned work".into(),
                message: "first assignment".into(),
            }),
        })
        .await
        .unwrap();
    assert_eq!(result["delivery"]["status"], "accepted");
    result["session_id"].as_str().unwrap().into()
}

async fn finish(control: &AgentControl, session: &str, event: EventType) {
    let snapshot = control
        .queries()
        .session_snapshot(session)
        .await
        .unwrap()
        .unwrap();
    let turn = control
        .queries()
        .list_turns(session)
        .await
        .unwrap()
        .last()
        .unwrap()
        .turn_id
        .clone();
    report(
        &control.event_ingest_service(),
        session,
        &snapshot.runtimes[0].runtime_id,
        &turn,
        EventType::TurnStarted,
    )
    .await;
    report(
        &control.event_ingest_service(),
        session,
        &snapshot.runtimes[0].runtime_id,
        &turn,
        event,
    )
    .await;
}

#[tokio::test]
async fn helpers_are_isolated_reusable_and_reply_to_the_current_bot() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (a, a_session) = bot(&conversation).await;
    let (b, b_session) = bot(&conversation).await;
    let first = helper(&helpers, &a_session).await;
    let second = helper(&helpers, &a_session).await;
    assert_ne!(first, second);
    assert_eq!(helpers.list(&a).await.unwrap().len(), 2);
    assert!(helpers.list(&b).await.unwrap().is_empty());
    assert_eq!(
        conversation.resolve(&first).await.unwrap().role,
        pontia_bot::identity::BotRole::Helper
    );
    for caller in [&b_session, &second] {
        assert!(
            helpers
                .execute(HelperRequest {
                    session_id: caller.clone(),
                    command: HelperCommand::Send {
                        session_id: first.clone(),
                        message: "forbidden".into(),
                    }
                })
                .await
                .is_err()
        );
    }
    assert!(
        helpers
            .execute(HelperRequest {
                session_id: a_session.clone(),
                command: HelperCommand::Reply {
                    message: "forbidden".into()
                }
            })
            .await
            .is_err()
    );
    finish(&control, &first, EventType::TurnCompleted).await;
    assert_eq!(
        helpers
            .list(&a)
            .await
            .unwrap()
            .iter()
            .find(|session| session.session_id == first)
            .unwrap()
            .state,
        "idle"
    );
    let response = helpers
        .execute(HelperRequest {
            session_id: a_session.clone(),
            command: HelperCommand::Send {
                session_id: first.clone(),
                message: "second assignment".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(response["delivery"]["status"], "accepted");
    assert_eq!(control.queries().list_turns(&first).await.unwrap().len(), 2);
    finish(&control, &a_session, EventType::TurnCompleted).await;
    conversation
        .send(&a, "user input while helpers work")
        .await
        .unwrap();
    finish(&control, &a_session, EventType::TurnCompleted).await;
    helpers
        .execute(HelperRequest {
            session_id: first.clone(),
            command: HelperCommand::Reply {
                message: "report wakes idle Bot".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        control
            .queries()
            .list_turns(&a_session)
            .await
            .unwrap()
            .len(),
        3
    );
    control
        .session_commands()
        .terminate_session(&a_session)
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_client_fact(
            &a_session,
            Some(
                &control
                    .queries()
                    .session_snapshot(&a_session)
                    .await
                    .unwrap()
                    .unwrap()
                    .runtimes[0]
                    .runtime_id,
            ),
            EventType::SessionExited,
            json!({"reason":"test client exit"}),
        )
        .await
        .unwrap();
    helpers
        .execute(HelperRequest {
            session_id: second.clone(),
            command: HelperCommand::Reply {
                message: "new generation report".into(),
            },
        })
        .await
        .unwrap();
    let current = conversation
        .records()
        .get(&a)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    assert_ne!(current, a_session);
    {
        let inputs = client.inputs.lock().unwrap();
        let latest = inputs.last().unwrap();
        assert_eq!(latest.session_id, current);
        assert!(latest.input.contains(&second));
        assert!(
            inputs
                .iter()
                .any(|input| input.session_id == first && input.input.contains(&a_session))
        );
    }
    let handoff = client.handoffs.lock().unwrap().last().unwrap().clone();
    assert!(handoff.contains(&first));
    assert!(handoff.contains(&second));
    helpers
        .execute(HelperRequest {
            session_id: current,
            command: HelperCommand::Close {
                session_id: first.clone(),
            },
        })
        .await
        .unwrap();
    let runtime = control
        .queries()
        .session_snapshot(&first)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    assert!(
        !GenericRuntimeManager.is_alive(&runtime),
        "close must release the actual runtime before an exit observation"
    );
    control
        .event_ingest_service()
        .report_client_fact(
            &first,
            Some(
                &control
                    .queries()
                    .session_snapshot(&first)
                    .await
                    .unwrap()
                    .unwrap()
                    .runtimes[0]
                    .runtime_id,
            ),
            EventType::SessionExited,
            json!({"reason":"test client exit"}),
        )
        .await
        .unwrap();
    assert_eq!(helpers.list(&a).await.unwrap().len(), 1);
    let closed = control
        .queries()
        .get_session(&first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(closed.state, "exited");
    assert!(
        !closed
            .runtimes
            .iter()
            .any(|runtime| runtime.state == "running")
    );
    assert_eq!(control.queries().list_turns(&first).await.unwrap().len(), 2);
}

#[tokio::test]
async fn helper_delivery_results_are_not_retried_and_failed_reports_survive_restart() {
    let (root, pool, client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (bot_id, session) = bot(&conversation).await;
    let helper = helper(&helpers, &session).await;
    for (message, expected) in [
        ("accepted input", "accepted"),
        ("reject once", "failed"),
        ("uncertain once", "unknown"),
    ] {
        for (caller, command) in [
            (
                session.clone(),
                HelperCommand::Send {
                    session_id: helper.clone(),
                    message: message.into(),
                },
            ),
            (
                helper.clone(),
                HelperCommand::Reply {
                    message: message.into(),
                },
            ),
        ] {
            let before = client.inputs.lock().unwrap().len();
            let result = helpers
                .execute(HelperRequest {
                    session_id: caller,
                    command,
                })
                .await
                .unwrap();
            assert_eq!(result["delivery"]["status"], expected);
            assert_eq!(client.inputs.lock().unwrap().len(), before + 1);
        }
    }
    *client.blocked.lock().unwrap() = Some(session.clone());
    helpers.reconcile().await.unwrap();
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered = 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 2);
    let original_count = client
        .inputs
        .lock()
        .unwrap()
        .iter()
        .filter(|input| input.input.starts_with("Source: Helper"))
        .count();
    let reopened = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("state.db").display()
    ))
    .await
    .unwrap();
    let recovered_control = AgentControl::builder(reopened, root.path().to_path_buf())
        .clients(control.clients())
        .build();
    let recovered = HelperService::new(
        recovered_control.clone(),
        ConversationService::new(recovered_control),
    );
    *client.blocked.lock().unwrap() = None;
    recovered.reconcile().await.unwrap();
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered = 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 0);
    let delivered = client.inputs.lock().unwrap().len();
    recovered.reconcile().await.unwrap();
    assert_eq!(client.inputs.lock().unwrap().len(), delivered);
    {
        let inputs = client.inputs.lock().unwrap();
        assert_eq!(
            inputs
                .iter()
                .filter(|input| input.input.starts_with("Source: Helper"))
                .count(),
            original_count
        );
        for (status, reason) in [("failed", "rejected"), ("unknown", "acknowledgement lost")] {
            assert!(inputs.iter().any(|input| input.session_id == session
                && input.input.starts_with("Source: System")
                && input.input.contains(&helper)
                && input.input.contains(status)
                && input.input.contains(reason)));
        }
    }
    assert_eq!(conversation.chat(&bot_id).await.unwrap().messages.len(), 1);
}

#[tokio::test]
async fn reliable_helper_failures_are_durable_and_event_replay_does_not_duplicate_notices() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (_, session) = bot(&conversation).await;
    let helper = helper(&helpers, &session).await;
    helpers.reconcile().await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 0);
    *client.blocked.lock().unwrap() = Some(session.clone());
    finish(&control, &helper, EventType::TurnFailed).await;
    helpers.reconcile().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered = 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
    sqlx::query("UPDATE bot_event_cursor SET position = 0")
        .execute(&pool)
        .await
        .unwrap();
    let recovered = HelperService::new(control.clone(), ConversationService::new(control.clone()));
    *client.blocked.lock().unwrap() = None;
    recovered.reconcile().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let inputs = client.inputs.lock().unwrap();
    assert!(inputs.last().unwrap().input.contains(&helper));
    assert!(inputs.last().unwrap().input.contains("turn.failed"));
}

#[tokio::test]
async fn running_bots_receive_new_workspaces_once_and_initial_context_does_not_wake_them() {
    use pontia_agent_control::workspaces::{RegisterWorkspaceRequest, WorkspaceService};
    let (root, _pool, client, control, conversation) = fixture().await;
    let workspace_service = WorkspaceService::new(
        control.db(),
        serde_json::from_value(json!({"roots":[{
            "root_id":"test", "label":"Test", "path":root.path()
        }]}))
        .unwrap(),
    );
    let register = |path: &str| RegisterWorkspaceRequest {
        root_id: "test".into(),
        path: path.into(),
        name: None,
        description: Some("project".into()),
    };
    std::fs::create_dir(root.path().join("existing")).unwrap();
    workspace_service
        .register(register("existing"))
        .await
        .unwrap();
    let (_, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let before = client.inputs.lock().unwrap().len();
    helpers.reconcile().await.unwrap();
    assert_eq!(client.inputs.lock().unwrap().len(), before);
    std::fs::create_dir(root.path().join("new")).unwrap();
    let workspace = workspace_service.register(register("new")).await.unwrap();
    helpers.reconcile().await.unwrap();
    {
        let inputs = client.inputs.lock().unwrap();
        assert_eq!(inputs.len(), before + 1);
        assert_eq!(inputs.last().unwrap().session_id, session);
        assert!(
            inputs
                .last()
                .unwrap()
                .input
                .contains(&workspace.workspace_id)
        );
    }
    workspace_service.register(register("new")).await.unwrap();
    let recovered = HelperService::new(control.clone(), ConversationService::new(control));
    recovered.reconcile().await.unwrap();
    assert_eq!(client.inputs.lock().unwrap().len(), before + 1);
}

#[tokio::test]
async fn helper_launch_pins_the_selected_profile_and_keeps_the_initialized_model() {
    use pontia_agent_control::{AgentProfileService, UpsertExecutionProfileRequest};
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    conversation
        .records()
        .update(&id, bot_configuration("changed", "missing/model"))
        .await
        .unwrap();
    let original = helper(&helpers, &session).await;
    let profiles = AgentProfileService::new(control.db()).with_clients(control.clients());
    let default_id = pontia_bot::records::BotService::helper_profile_id(&id);
    let default = profiles.get_latest(&default_id).await.unwrap().unwrap();
    let mut request: UpsertExecutionProfileRequest =
        serde_json::from_value(json!(default)).unwrap();
    request.profile_id = "specialist".into();
    request.version = "2".into();
    request.system_prompt_template = Some("Review only".into());
    profiles.create_profile(request.clone()).await.unwrap();
    let response = helpers
        .execute(HelperRequest {
            session_id: session.clone(),
            command: HelperCommand::Create(CreateHelper {
                profile_id: Some("specialist".into()),
                profile_version: Some("2".into()),
                description: "specialized task".into(),
                message: "task".into(),
            }),
        })
        .await
        .unwrap();
    let target = response["session_id"].as_str().unwrap();
    request.version = "3".into();
    request.default_model = Some("missing/model".into());
    profiles
        .create_profile_version("specialist", request)
        .await
        .unwrap();
    let created = control
        .queries()
        .get_session(target)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created.execution_profile_id.as_deref(), Some("specialist"));
    assert_eq!(created.execution_profile_version.as_deref(), Some("2"));
    assert_eq!(
        control
            .queries()
            .get_session(&original)
            .await
            .unwrap()
            .unwrap()
            .execution_profile_id
            .as_deref(),
        Some(default_id.as_str())
    );
}

#[path = "conversation/retrieval.rs"]
mod retrieval;

#[path = "conversation/dreaming.rs"]
mod dreaming;
