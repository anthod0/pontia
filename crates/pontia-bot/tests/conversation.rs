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
        let failure = match input.input.lines().last() {
            Some("reject once") if first => Err(Error::Domain("rejected".into())),
            Some("uncertain once") if first => {
                Err(Error::ControlUnknown("acknowledgement lost".into()))
            }
            _ => Ok(()),
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
