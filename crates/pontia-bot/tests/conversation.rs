use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use pontia_agent_control::{
    AgentControl, EventIngestService, ReportedFact, UpsertAgentProfileRequest,
    agent_clients::AgentClientService,
    client_contract::{
        AgentInput, ClientConfiguration, ClientExitMode, ClientExitOutcome, ClientInteractions,
        ClientLaunchRequest, ClientOperation, ClientSession, ClientSessionDetails,
        DeliveryOperation, GenericTestClient, InputPayload, test_registration,
    },
    clients::ClientRegistry,
    control::InputReceipt,
    interactions::InteractionResponse,
    runtime::ControlTarget,
};
use pontia_bot::{
    conversation::{ConversationService, ConversationUpdate, Reply, ReplyRequest, WorkStatus},
    identity::{BotIdentity, TOOL_TOKEN_ENV},
    messages::MessageStatus,
    records::BotConfiguration,
};
use pontia_core::{
    Error, LaunchMode, Result,
    client_capabilities::AgentClientCapabilities,
    domain::{EventType, ReportedEvent},
};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult};
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
    steer: Mutex<Option<bool>>,
    reject_tasks: Mutex<bool>,
    blocked: Mutex<Option<String>>,
    unavailable: Mutex<std::collections::HashSet<String>>,
    all_unavailable: Mutex<bool>,
    launch_gate: Mutex<
        Option<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
    tokens: Mutex<std::collections::HashMap<String, String>>,
    reject_restricted: Mutex<bool>,
    launch_directories: Mutex<std::collections::HashMap<String, String>>,
    fail_next_launch: Mutex<bool>,
    launch_modes: Mutex<std::collections::HashMap<String, LaunchMode>>,
    launched_efforts: Mutex<std::collections::HashMap<String, Option<String>>>,
    launched_models: Mutex<std::collections::HashMap<String, Option<String>>>,
    session_modes: Mutex<std::collections::HashMap<String, pontia_core::SessionMode>>,
    interaction_responses: Mutex<Vec<(String, String, String)>>,
    fail_interaction_response: Mutex<bool>,
}

impl ClientConfiguration for Client {
    fn supports_mode(&self, key: &str, mode: pontia_core::SessionMode) -> bool {
        key == "default"
            && (mode == pontia_core::SessionMode::Default
                || !*self.reject_restricted.lock().unwrap())
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
        true
    }
    fn list_models<'a>(
        &'a self,
        _: &'a Value,
        _: &'a Path,
    ) -> ClientOperation<'a, pontia_agent_control::agent_clients::AgentClientModels> {
        Box::pin(async {
            Ok(pontia_agent_control::agent_clients::AgentClientModels {
                models: vec![pontia_agent_control::sessions::SessionModel {
                    id: MODEL.into(),
                    name: MODEL.into(),
                    description: String::new(),
                    default_reasoning_effort: None,
                    reasoning_efforts: ["low", "high"]
                        .map(|id| pontia_agent_control::sessions::ReasoningEffortOption {
                            id: id.into(),
                            name: id.into(),
                            description: String::new(),
                        })
                        .into(),
                }],
                default_model: Some(MODEL.into()),
            })
        })
    }
    fn supports_startup_model(&self) -> bool {
        true
    }
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, key: &str, _: &Value) -> (String, String) {
        (key.into(), key.into())
    }
    fn key(&self, _: &Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _: &str) -> Value {
        json!({})
    }
    fn resolve(&self, _: &str) -> Result<Value> {
        Ok(json!({}))
    }
}

impl Client {
    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult> {
        self.launch_modes.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.launch_mode,
        );
        self.launched_efforts.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.reasoning_effort.clone(),
        );
        self.launched_models.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.model.clone(),
        );
        self.session_modes
            .lock()
            .unwrap()
            .insert(request.runtime.session_id.clone(), request.runtime.mode);
        self.launch_directories.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.workspace.clone().unwrap(),
        );
        if std::mem::take(&mut *self.fail_next_launch.lock().unwrap()) {
            return Err(Error::Unavailable("injected launch failure".into()));
        }
        let gate = self.launch_gate.lock().unwrap().take();
        if let Some((entered, release)) = gate {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        if request.runtime.model.as_deref() != Some(MODEL) {
            return Err(Error::Unavailable("model unavailable".into()));
        }
        self.tokens.lock().unwrap().insert(
            request.runtime.session_id.clone(),
            request.runtime.environment[TOOL_TOKEN_ENV].clone(),
        );
        GenericRuntimeManager.start_in_process(request.root, request.runtime, CAPABILITIES, 0)
    }
}

impl ClientSession for Client {
    fn provision<'a>(
        &'a self,
        _events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move {
            self.launch(ClientLaunchRequest {
                root,
                runtime: request,
                restart_count: 0,
                reuse_pane: None,
                native_session_key: None,
                native_session_file: None,
            })
            .map(Some)
        })
    }
    fn resume_with_runtime<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        request: ClientLaunchRequest<'a>,
    ) -> ClientOperation<'a, Option<RuntimeStartResult>> {
        Box::pin(async move { self.launch(request).map(Some) })
    }
    fn start_turn<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            self.submit(AgentInput {
                session_id: target.session_id.clone(),
                dispatch_id: pontia_core::ids::new_dispatch_id().to_string(),
                input: payload.text.clone(),
            })
            .map_err(|error| match error {
                Error::Domain(message) => pontia_agent_control::DeliveryError::Rejected(message),
                error => pontia_agent_control::DeliveryError::from(error),
            })?;
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }
    fn steer<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
    ) -> DeliveryOperation<'a, InputReceipt> {
        self.start_turn(events, target, payload, None)
    }
    fn exit<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        _mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            GenericRuntimeManager.terminate_session(target.instance()?)?;
            Ok(ClientExitOutcome::Requested)
        })
    }
    fn available<'a>(
        &'a self,
        _store: pontia_agent_control::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn capability_state<'a>(
        &'a self,
        _: pontia_agent_control::ClientControlStore,
        _session: &'a str,
        _implementation: &'a AgentClientCapabilities,
    ) -> ClientOperation<'a, pontia_agent_control::client_contract::SessionCapabilityState> {
        Box::pin(async move { Ok(self.capabilities().into()) })
    }
    fn ready(&self, session: &str, instance: &str) -> Option<ReportedEvent> {
        Some(GenericTestClient::ready_event(session, instance))
    }
    fn details<'a>(
        &'a self,
        _store: pontia_agent_control::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async { Ok(ClientSessionDetails { data: json!({}) }) })
    }
}

impl pontia_agent_control::client_contract::ClientInstructions for Client {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }
    fn bind(
        &self,
        input: &pontia_agent_control::client_contract::InstructionsBindingInput<'_>,
    ) -> Result<Value> {
        Ok(json!({"system": input.system_prompt}))
    }
}

impl ClientInteractions for Client {
    fn resolve<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        response: &'a InteractionResponse,
    ) -> ClientOperation<'a, ()> {
        let option = response.choice_option_key().unwrap();
        self.interaction_responses.lock().unwrap().push((
            session.to_owned(),
            interaction.to_owned(),
            option.to_owned(),
        ));
        let fail = *self.fail_interaction_response.lock().unwrap();
        Box::pin(async move {
            if fail {
                return Err(Error::ControlUnknown(
                    "interaction channel unavailable".into(),
                ));
            }
            events
                .report_client_fact(
                    session,
                    None,
                    EventType::InteractionUnblocked,
                    json!({"interaction_id": interaction}),
                )
                .await?;
            Ok(())
        })
    }
}

impl Client {
    fn capabilities(&self) -> AgentClientCapabilities {
        AgentClientCapabilities {
            accept_task: !*self.reject_tasks.lock().unwrap(),
            steer: self.steer.lock().unwrap().unwrap_or(CAPABILITIES.steer),
            ..CAPABILITIES
        }
    }
    fn submit(&self, input: AgentInput) -> Result<()> {
        let mut inputs = self.inputs.lock().unwrap();
        let first = !inputs.iter().any(|previous| previous.input == input.input);
        let failure = if self.blocked.lock().unwrap().as_deref() == Some(&input.session_id) {
            Err(Error::ControlUnknown("connection lost".into()))
        } else if *self.all_unavailable.lock().unwrap()
            || self.unavailable.lock().unwrap().contains(&input.session_id)
        {
            Err(Error::Unavailable("session history is corrupted".into()))
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
        avatar_shape: None,
        avatar_color: None,
        client_type: "generic".into(),
        client_key: "default".into(),
        model: model.into(),
        reasoning_effort: None,
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
        .create(bot_configuration("A", "missing/model"), Default::default())
        .await
        .unwrap();
    let a = &a.bot_id;
    let failed = service.send(a, "historical failure").await.unwrap().message;
    assert_eq!(failed.status, MessageStatus::Failed);
    service
        .records()
        .update(a, bot_configuration("A", MODEL))
        .await
        .unwrap();
    let b = service
        .records()
        .create(bot_configuration("B", MODEL), Default::default())
        .await
        .unwrap();
    let b = &b.bot_id;
    let wake = service.send(a, "wake A").await.unwrap().message;
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
    assert_launch_mode(&control, &client, &a_session, LaunchMode::Background).await;
    assert_launch_mode(&control, &client, &b_session, LaunchMode::Background).await;
    let workspaces = control.queries().list_workspaces().await.unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(
        Path::new(&workspaces[0].canonical_path),
        _root.path().join("Pontia").canonicalize().unwrap()
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
        let failed = service.send(a, content).await.unwrap().message;
        assert_eq!(failed.status, MessageStatus::Failed);
        assert!(service.retry(b, &failed.id).await.is_err());
        let retried = service.retry(a, &failed.id).await.unwrap().message;
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
        !control
            .queries()
            .session_snapshot(&a_session)
            .await
            .unwrap()
            .unwrap()
            .has_unsettled_input
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

    let reply = |session: &str, text: &str| ReplyRequest {
        session_id: session.into(),
        input: Reply {
            todo: false,
            resolve_todos: vec![],
            related_to: vec![],
            message: text.into(),
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
            .send_to_user(reply("foreign", "must not show"))
            .await
            .is_err()
    );
    let mut updates = service.subscribe(a);
    service
        .send_to_user(reply(&a_session, "Complete\nreply"))
        .await
        .unwrap();
    assert!(matches!(
        updates.recv().await.unwrap(),
        ConversationUpdate::Typing
    ));
    assert!(matches!(
        updates.recv().await.unwrap(),
        ConversationUpdate::Message { message } if message.content == "Complete\nreply"
    ));
    let chat = service.chat(a).await.unwrap();
    assert_eq!(chat.messages.last().unwrap().content, "Complete\nreply");

    let restarted = test_conversation(control.clone());
    assert_eq!(restarted.chat(a).await.unwrap().status, WorkStatus::Idle);
    service
        .send_to_user(reply(&a_session, "Done"))
        .await
        .unwrap();
    assert_eq!(service.chat(a).await.unwrap().status, WorkStatus::Idle);
    assert_eq!(service.chat(b).await.unwrap().messages.len(), 2);

    let a_handoff = instructions(&control, &a_session).await;
    let b_handoff = instructions(&control, &b_session).await;
    assert!(a_handoff.contains("historical failure"));
    assert!(!a_handoff.contains("wake A"));
    assert!(!b_handoff.contains("historical failure"));
    let delivered = client.inputs.lock().unwrap().len();
    let recovered = test_conversation(control);
    let chat = recovered.chat(a).await.unwrap();
    assert_eq!(chat.messages[1].status, MessageStatus::Failed);
    assert_eq!(client.inputs.lock().unwrap().len(), delivered);
    let inputs = client.inputs.lock().unwrap();
    assert_eq!(
        inputs[0].input,
        format!("Source: User\nMessage: {}\n\nwake A", wake.id)
    );
    assert_eq!(inputs[0].session_id, a_session);
    assert_eq!(inputs[1].session_id, b_session);
    assert!(
        inputs
            .iter()
            .all(|input| !input.input.contains("historical failure"))
    );
}

#[tokio::test]
async fn failed_messages_show_processing_only_after_a_successful_retry() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(
            bot_configuration("Bot", "missing/model"),
            Default::default(),
        )
        .await
        .unwrap();
    let failed = service.send(&bot.bot_id, "hello").await.unwrap().message;
    assert_eq!(failed.status, MessageStatus::Failed);
    assert!(service.chat(&bot.bot_id).await.unwrap().status != WorkStatus::Processing);

    service
        .records()
        .update(&bot.bot_id, bot_configuration("Bot", MODEL))
        .await
        .unwrap();
    service.retry(&bot.bot_id, &failed.id).await.unwrap();
    assert!(service.chat(&bot.bot_id).await.unwrap().status == WorkStatus::Processing);
}

#[tokio::test]
async fn unavailable_session_is_replaced_and_the_input_redelivered() {
    let (_root, _pool, client, control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let id = bot.bot_id;
    let current = || async { service.records().get(&id).await.unwrap().current_session_id };
    let terminated = |session: String| {
        let control = control.clone();
        async move {
            control
                .queries()
                .session_snapshot(&session)
                .await
                .unwrap()
                .unwrap()
                .runtimes
                .iter()
                .all(|runtime| !GenericRuntimeManager.is_alive(&runtime.runtime_id))
        }
    };
    service.send(&id, "wake").await.unwrap();
    let broken = current().await.unwrap();
    client.unavailable.lock().unwrap().insert(broken.clone());

    let sent = service.send(&id, "after corruption").await.unwrap().message;
    assert_eq!(sent.status, MessageStatus::Normal);
    let replacement = current().await.unwrap();
    assert_ne!(replacement, broken);
    assert!(terminated(broken).await);
    let last = client.inputs.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.session_id, replacement);
    assert!(last.input.ends_with("after corruption"));

    client
        .unavailable
        .lock()
        .unwrap()
        .insert(replacement.clone());
    service
        .deliver_input(&id, "Source: System Notification\nRecovered")
        .await
        .unwrap();
    let notified = current().await.unwrap();
    assert_ne!(notified, replacement);
    assert!(terminated(replacement).await);
    let last = client.inputs.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.session_id, notified);
    assert!(last.input.ends_with("Recovered"));
}

#[tokio::test]
async fn unavailable_replacement_session_fails_the_input_without_another_rebuild() {
    let (_root, _pool, client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let id = bot.bot_id;
    service.send(&id, "wake").await.unwrap();
    *client.all_unavailable.lock().unwrap() = true;
    let launches = client.launch_modes.lock().unwrap().len();

    let failed = service.send(&id, "still broken").await.unwrap().message;
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(client.launch_modes.lock().unwrap().len(), launches + 1);
}

#[tokio::test]
async fn unsupported_input_keeps_the_current_bot_session() {
    let (_root, _pool, client, control, service) = fixture().await;
    let (id, session) = bot(&service).await;
    finish(&control, &session, EventType::TurnCompleted).await;
    *client.reject_tasks.lock().unwrap() = true;
    let launches = client.launch_modes.lock().unwrap().len();

    let failed = service
        .send(&id, "unsupported input")
        .await
        .unwrap()
        .message;
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(client.launch_modes.lock().unwrap().len(), launches);
    assert_eq!(
        service.records().get(&id).await.unwrap().current_session_id,
        Some(session)
    );
}

#[tokio::test]
async fn unavailable_client_keeps_the_current_session() {
    let (_root, _pool, client, _control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let id = bot.bot_id;
    service.send(&id, "wake").await.unwrap();
    let session = service.records().get(&id).await.unwrap().current_session_id;
    *client.all_unavailable.lock().unwrap() = true;
    *client.reject_restricted.lock().unwrap() = true;
    let launches = client.launch_modes.lock().unwrap().len();

    let failed = service.send(&id, "client removed").await.unwrap().message;
    assert_eq!(failed.status, MessageStatus::Failed);
    assert_eq!(client.launch_modes.lock().unwrap().len(), launches);
    assert_eq!(
        service.records().get(&id).await.unwrap().current_session_id,
        session
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn processing_shows_until_the_bot_answers_or_its_turn_ends() {
    let (_root, _pool, client, control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    let id = bot.bot_id.clone();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, released) = std::sync::mpsc::channel();
    *client.launch_gate.lock().unwrap() = Some((entered, released));
    let sender = service.clone();
    let target = id.clone();
    let sending = tokio::spawn(async move { sender.send(&target, "first").await.unwrap() });
    waiting.await.unwrap();
    assert!(service.chat(&id).await.unwrap().status == WorkStatus::Processing);
    release.send(()).unwrap();
    sending.await.unwrap();
    assert!(service.chat(&id).await.unwrap().status == WorkStatus::Processing);

    let session = service
        .records()
        .get(&id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let reply = |text: &str| ReplyRequest {
        session_id: session.clone(),
        input: Reply {
            todo: false,
            resolve_todos: vec![],
            related_to: vec![],
            message: text.into(),
        },
    };
    service.send_to_user(reply("Hi")).await.unwrap();
    assert!(service.chat(&id).await.unwrap().status != WorkStatus::Processing);

    service.send(&id, "second").await.unwrap();
    assert!(service.chat(&id).await.unwrap().status == WorkStatus::Processing);
    service.send_to_user(reply("On it")).await.unwrap();
    assert!(service.chat(&id).await.unwrap().status != WorkStatus::Processing);
    service.send(&id, "third").await.unwrap();
    let chat = service.chat(&id).await.unwrap();
    assert!(chat.status == WorkStatus::Processing);

    service.send_to_user(reply("Still checking")).await.unwrap();
    assert!(service.chat(&id).await.unwrap().status != WorkStatus::Processing);

    service.send_to_user(reply("Done")).await.unwrap();
    service.send(&id, "fourth").await.unwrap();
    assert!(service.chat(&id).await.unwrap().status == WorkStatus::Processing);
    finish(&control, &session, EventType::TurnCompleted).await;
    assert!(service.chat(&id).await.unwrap().status != WorkStatus::Processing);
}

#[tokio::test]
async fn busy_bot_without_steering_queues_follow_up_input() {
    let (_root, _pool, client, control, service) = fixture().await;
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
        .await
        .unwrap();
    service.send(&bot.bot_id, "first").await.unwrap();
    *client.steer.lock().unwrap() = Some(false);

    let delivery = service
        .deliver_input(&bot.bot_id, "follow up")
        .await
        .unwrap();
    assert_eq!(serde_json::to_value(delivery).unwrap(), "queued");
    assert_eq!(client.inputs.lock().unwrap().len(), 1);
    let session = service
        .records()
        .get(&bot.bot_id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let messages = control
        .inbox_commands()
        .list_messages(&session)
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        service.chat(&bot.bot_id).await.unwrap().status,
        WorkStatus::Processing
    );
}

async fn instructions(control: &AgentControl, session: &str) -> String {
    pontia_agent_control::SessionInstructionsService::new(control.db())
        .session_instructions(session)
        .await
        .unwrap()
        .unwrap()["system"]
        .as_str()
        .unwrap()
        .to_owned()
}

struct ExitReporter(tokio::task::JoinHandle<()>);
impl Drop for ExitReporter {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn report_exits(control: &AgentControl) -> ExitReporter {
    let control = control.clone();
    ExitReporter(tokio::spawn(async move {
        loop {
            for session in control.queries().list_owned_sessions("bot").await.unwrap() {
                let snapshot = control
                    .queries()
                    .session_snapshot(&session)
                    .await
                    .unwrap()
                    .unwrap();
                if snapshot.state != "exited" {
                    for runtime in &snapshot.runtimes {
                        if !GenericRuntimeManager.is_alive(&runtime.runtime_id) {
                            control
                                .event_ingest_service()
                                .report_client_fact(
                                    &session,
                                    Some(&runtime.runtime_id),
                                    EventType::SessionExited,
                                    json!({"reason":"test runtime exited"}),
                                )
                                .await
                                .unwrap();
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }))
}

const BOT_CLIENT: pontia_agent_control::client_contract::AgentClientSpec =
    pontia_agent_control::client_contract::AgentClientSpec {
        capabilities: CAPABILITIES,
        ..pontia_agent_control::client_contract::TEST_SPEC
    };

type Fixture = (
    tempfile::TempDir,
    sqlx::SqlitePool,
    Arc<Client>,
    AgentControl,
    ConversationService,
);

fn test_conversation(control: AgentControl) -> ConversationService {
    let home = control.pontia_home().to_owned();
    let roots = vec![pontia_config::WorkspaceRootConfig {
        root_id: "projects".into(),
        label: "My projects".into(),
        path: home.join("projects").display().to_string(),
    }];
    ConversationService::new(control, Some(home), roots)
}

async fn fixture() -> Fixture {
    fixture_with_client(&BOT_CLIENT).await
}

async fn fixture_with_client(
    spec: &'static pontia_agent_control::client_contract::AgentClientSpec,
) -> Fixture {
    fixture_with_clients(&[spec]).await
}

async fn fixture_with_clients(
    specs: &[&'static pontia_agent_control::client_contract::AgentClientSpec],
) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("state.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let client = Arc::new(Client::default());
    let mut registry = ClientRegistry::default();
    for spec in specs {
        let mut registration = test_registration();
        registration.spec = spec;
        registration.configuration = Some(client.clone());
        registration.interactions = Some(client.clone());
        registration.session = Some(client.clone());
        registration.instructions = Some(client.clone());
        registry.register(registration);
    }
    for spec in specs {
        AgentClientService::new(pool.clone(), registry.clone())
            .save(spec.client_type, "default", true)
            .await
            .unwrap();
    }
    let control = AgentControl::builder(pool.clone(), root.path().to_owned())
        .managed_private_root(root.path().join("bot"))
        .clients(registry)
        .build();
    let service = test_conversation(control.clone());

    (root, pool, client, control, service)
}

use pontia_bot::helpers::{
    CreateHelper, HelperCommand, HelperLocation, HelperRequest, HelperService,
};

async fn assert_launch_mode(
    control: &AgentControl,
    client: &Client,
    session: &str,
    expected: LaunchMode,
) {
    assert_eq!(
        client.launch_modes.lock().unwrap().get(session),
        Some(&expected)
    );
    assert_eq!(
        control
            .queries()
            .session_launch_mode(session)
            .await
            .unwrap(),
        expected
    );
}

async fn bot(service: &ConversationService) -> (String, String) {
    let bot = service
        .records()
        .create(bot_configuration("Bot", MODEL), Default::default())
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
                location: HelperLocation::Temporary("2026-11-20_task".into()),
                profile_id: None,
                title: "Assigned work".into(),
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

async fn wait_for_exit(control: &AgentControl, session: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if control
                .queries()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .state
                == "exited"
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
    assert_launch_mode(&control, &client, &current, LaunchMode::Background).await;
    {
        let inputs = client.inputs.lock().unwrap();
        let latest = inputs.last().unwrap();
        assert_eq!(latest.session_id, current);
        assert!(latest.input.contains(&second));
        assert!(
            inputs.iter().any(|input| input.session_id == first
                && input.input == "Source: Bot\n\nsecond assignment")
        );
    }
    let handoff = instructions(&control, &current).await;
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
async fn helper_remains_addressable_when_first_delivery_fails_or_is_unknown() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let (bot_id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control, conversation);
    for (task, status) in [("reject once", "failed"), ("uncertain once", "unknown")] {
        let response = helpers
            .execute(HelperRequest {
                session_id: session.clone(),
                command: HelperCommand::Create(CreateHelper {
                    location: HelperLocation::Temporary("2026-11-20_task".into()),
                    profile_id: None,
                    title: "Assigned work".into(),
                    description: task.into(),
                    message: task.into(),
                }),
            })
            .await
            .unwrap();
        assert_eq!(response["delivery"]["status"], status);
        let id = response["session_id"].as_str().unwrap();
        assert!(
            helpers
                .list(&bot_id)
                .await
                .unwrap()
                .iter()
                .any(|helper| helper.session_id == id)
        );
        let inputs = client.inputs.lock().unwrap();
        let mut delivered = inputs.iter().filter(|input| input.session_id == id);
        let input = &delivered.next().unwrap().input;
        assert!(input.starts_with("Source: Bot\n\n"));
        assert!(input.ends_with(&format!("\n\n{task}")));
        assert!(delivered.next().is_none());
    }
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
            let expected_input = if caller == session {
                format!("Source: Bot\n\n{message}")
            } else {
                format!("Source: Helper\nSession: {helper}\n\n{message}")
            };
            let before = client.inputs.lock().unwrap().len();
            let result = helpers
                .execute(HelperRequest {
                    session_id: caller,
                    command,
                })
                .await
                .unwrap();
            assert_eq!(result["delivery"]["status"], expected);
            let inputs = client.inputs.lock().unwrap();
            assert_eq!(inputs.len(), before + 1);
            assert_eq!(inputs.last().unwrap().input, expected_input);
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
        .managed_private_root(root.path().join("bot"))
        .clients(control.clients())
        .build();
    let recovered = HelperService::new(
        recovered_control.clone(),
        test_conversation(recovered_control),
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
                && input.input.starts_with("Source: System Notification\n")
                && input.input.contains(&helper)
                && input.input.contains(status)
                && input.input.contains(reason)));
        }
    }
    assert_eq!(conversation.chat(&bot_id).await.unwrap().messages.len(), 2);
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
    let snapshot = control
        .queries()
        .session_snapshot(&helper)
        .await
        .unwrap()
        .unwrap();
    let turns = control.queries().list_turns(&helper).await.unwrap();
    report(
        &control.event_ingest_service(),
        &helper,
        &snapshot.runtimes[0].runtime_id,
        &turns.last().unwrap().turn_id,
        EventType::TurnFailed,
    )
    .await;
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
    let recovered = HelperService::new(control.clone(), test_conversation(control.clone()));
    *client.blocked.lock().unwrap() = None;
    recovered.reconcile().await.unwrap();
    let contents: Vec<String> = sqlx::query_scalar("SELECT content FROM bot_notifications")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(contents.len(), 1);
    let content = &contents[0];
    assert!(content.contains("turn.failed"));
    {
        let inputs = client.inputs.lock().unwrap();
        assert_eq!(
            inputs.last().unwrap().input,
            format!("Source: System Notification\nHelper Session: {helper}\n{content}")
        );
    }
    for _ in 0..2 {
        control
            .event_ingest_service()
            .report_client_fact(
                &helper,
                Some(&snapshot.runtimes[0].runtime_id),
                EventType::SessionExited,
                json!({"reason":"quit"}),
            )
            .await
            .unwrap();
        recovered.reconcile().await.unwrap();
    }
    let notices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bot_notifications WHERE source_session_id=? AND delivered=1",
    )
    .bind(&helper)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(notices, 2);
}

#[tokio::test]
async fn failed_notification_delivery_waits_for_backoff_instead_of_retrying_immediately() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (_, session) = bot(&conversation).await;
    let helper = helper(&helpers, &session).await;
    *client.blocked.lock().unwrap() = Some(session.clone());
    finish(&control, &helper, EventType::TurnFailed).await;
    let snapshot = control
        .queries()
        .session_snapshot(&helper)
        .await
        .unwrap()
        .unwrap();
    let turns = control.queries().list_turns(&helper).await.unwrap();
    report(
        &control.event_ingest_service(),
        &helper,
        &snapshot.runtimes[0].runtime_id,
        &turns.last().unwrap().turn_id,
        EventType::TurnFailed,
    )
    .await;
    let attempts = || {
        client
            .inputs
            .lock()
            .unwrap()
            .iter()
            .filter(|input| {
                input.session_id == session
                    && input.input.starts_with("Source: System Notification\n")
            })
            .count()
    };
    helpers.reconcile().await.unwrap();
    assert_eq!(attempts(), 1);
    *client.blocked.lock().unwrap() = None;
    for _ in 0..3 {
        helpers.reconcile().await.unwrap();
    }
    assert_eq!(attempts(), 1);
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered = 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 1);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    helpers.reconcile().await.unwrap();
    assert_eq!(attempts(), 2);
    let pending: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM bot_notifications WHERE delivered = 0")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 0);
}

#[path = "conversation/system_notices.rs"]
mod system_notices;

fn profile(id: &str, model: &str, system_prompt: Option<&str>) -> UpsertAgentProfileRequest {
    UpsertAgentProfileRequest {
        use_yolo: false,
        profile_id: id.into(),
        name: id.into(),
        description: None,
        client_type: Some("generic".into()),
        client_key: Some("default".into()),
        model: Some(model.into()),
        reasoning_effort: None,
        system_prompt: system_prompt.map(Into::into),
    }
}

async fn create_helper(
    service: &HelperService,
    session: &str,
    profile_id: Option<&str>,
) -> std::result::Result<Value, pontia_bot::helpers::HelperError> {
    service
        .execute(HelperRequest {
            session_id: session.into(),
            command: HelperCommand::Create(CreateHelper {
                location: HelperLocation::Temporary("2026-11-20_task".into()),
                profile_id: profile_id.map(Into::into),
                title: "Assigned work".into(),
                description: "assigned work".into(),
                message: "task".into(),
            }),
        })
        .await
}

fn launched_model(client: &Client, session: &str) -> Option<String> {
    client.launched_models.lock().unwrap()[session].clone()
}

const HELPER_INSTRUCTIONS: &str = include_str!("../src/helpers/instructions.md");

#[tokio::test]
async fn general_helper_follows_a_corrected_bot_model_and_ignores_legacy_helper_profiles() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let bot = conversation
        .records()
        .create(
            bot_configuration("Bot", "missing/model"),
            Default::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        conversation
            .send(&bot.bot_id, "start")
            .await
            .unwrap()
            .message
            .status,
        MessageStatus::Failed
    );
    control
        .agent_profiles()
        .create(
            profile(
                &format!("{}-helper", bot.bot_id),
                "missing/model",
                Some("legacy prompt"),
            ),
            None,
        )
        .await
        .unwrap();
    conversation
        .records()
        .update(
            &bot.bot_id,
            BotConfiguration {
                reasoning_effort: Some("low".into()),
                ..bot_configuration("Bot", MODEL)
            },
        )
        .await
        .unwrap();
    conversation.send(&bot.bot_id, "retry").await.unwrap();
    let session = conversation
        .records()
        .get(&bot.bot_id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let helpers = HelperService::new(control.clone(), conversation.clone());
    assert_eq!(
        client.launched_efforts.lock().unwrap()[&session].as_deref(),
        Some("low")
    );
    let response = create_helper(&helpers, &session, None).await.unwrap();
    let helper = response["session_id"].as_str().unwrap();
    assert_eq!(launched_model(&client, helper), Some(MODEL.into()));
    assert_eq!(
        client.launched_efforts.lock().unwrap()[helper].as_deref(),
        Some("low")
    );
    assert_eq!(instructions(&control, helper).await, HELPER_INSTRUCTIONS);
}

#[tokio::test]
async fn general_helper_launches_with_the_current_bot_client_key() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let mut changed = bot_configuration("Bot", MODEL);
    changed.client_key = "unconfigured".into();
    conversation.records().update(&id, changed).await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(
        create_helper(&helpers, &session, None).await,
        Err(pontia_bot::helpers::HelperError::Control(Error::Domain(_)))
    ));
    conversation
        .records()
        .update(&id, bot_configuration("Bot", MODEL))
        .await
        .unwrap();
    create_helper(&helpers, &session, None).await.unwrap();
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before + 1);
}

#[tokio::test]
async fn bot_and_profile_efforts_must_be_offered_by_the_selected_model() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let invalid = BotConfiguration {
        reasoning_effort: Some("missing".into()),
        ..bot_configuration("Bot", MODEL)
    };
    assert!(matches!(
        conversation.records().update(&id, invalid).await,
        Err(Error::Domain(_))
    ));
    assert!(
        conversation
            .records()
            .get(&id)
            .await
            .unwrap()
            .reasoning_effort
            .is_none()
    );
    let profiles = control.agent_profiles();
    profiles
        .create(profile("reviewer", MODEL, None), None)
        .await
        .unwrap();
    let update = |fields: Value| {
        let mut input = fields;
        input["profile_id"] = json!("reviewer");
        serde_json::from_value(
            json!({"session_id":session, "command":{"action":"update","input":input}}),
        )
        .unwrap()
    };
    assert!(matches!(
        conversation
            .profile_command(update(json!({"reasoning_effort":"missing"})))
            .await,
        Err(Error::Domain(_))
    ));
    for (fields, effort) in [
        (json!({"reasoning_effort":"high"}), Some("high")),
        (json!({"name":"Reviewer"}), Some("high")),
        (json!({"clear_reasoning_effort":true}), None),
    ] {
        conversation.profile_command(update(fields)).await.unwrap();
        assert_eq!(
            profiles
                .resolve_launch("reviewer")
                .await
                .unwrap()
                .reasoning_effort
                .as_deref(),
            effort
        );
    }
}

#[tokio::test]
async fn profile_commands_preserve_yolo_until_explicitly_disabled() {
    let (_root, _pool, _client, _control, service) = fixture().await;
    let (_, session) = bot(&service).await;
    let command =
        |command| serde_json::from_value(json!({"session_id":session,"command":command})).unwrap();
    let create = json!({"action":"create","input":{"profile_id":"writer","name":"Writer","description":"Write notes","client":{"client_type":"generic","client_key":"default"},"model":MODEL,"use_yolo":true}});
    assert_eq!(
        service.profile_command(command(create)).await.unwrap()["profile"]["use_yolo"],
        true
    );
    for (fields, expected) in [
        (json!({"name":"Editor"}), true),
        (json!({"use_yolo":false}), false),
    ] {
        let mut input = fields;
        input["profile_id"] = json!("writer");
        service
            .profile_command(command(json!({"action":"update","input":input})))
            .await
            .unwrap();
        let show = service
            .profile_command(command(
                json!({"action":"show","input":{"profile_id":"writer"}}),
            ))
            .await
            .unwrap();
        assert_eq!(show["profile"]["use_yolo"], expected);
        let list = service
            .profile_command(command(json!({"action":"list"})))
            .await
            .unwrap();
        assert_eq!(list["profiles"][0]["use_yolo"], expected);
    }
}

#[tokio::test]
async fn profile_helper_uses_the_profile_launch_config_and_keeps_its_instruction_snapshot() {
    let (_root, _pool, client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let profiles = control.agent_profiles();
    profiles
        .create(
            UpsertAgentProfileRequest {
                reasoning_effort: Some("high".into()),
                use_yolo: true,
                ..profile("specialist", MODEL, Some("Review only"))
            },
            None,
        )
        .await
        .unwrap();
    let mut changed = bot_configuration("changed", "missing/model");
    changed.client_key = "unconfigured".into();
    conversation.records().update(&id, changed).await.unwrap();
    let response = create_helper(&helpers, &session, Some("specialist"))
        .await
        .unwrap();
    let target = response["session_id"].as_str().unwrap();
    assert_eq!(
        client.session_modes.lock().unwrap()[target],
        pontia_core::SessionMode::Yolo
    );
    assert_eq!(launched_model(&client, target), Some(MODEL.into()));
    assert_eq!(
        client.launched_efforts.lock().unwrap()[target].as_deref(),
        Some("high")
    );
    let binding = instructions(&control, target).await;
    assert!(
        !binding.contains(pontia_agent_control::standalone_session_instructions()),
        "helper session must not receive standalone session instructions"
    );
    assert!(
        !instructions(&control, &session)
            .await
            .contains(pontia_agent_control::standalone_session_instructions()),
        "bot session must not receive standalone session instructions"
    );
    assert_eq!(binding.matches("Review only").count(), 1);
    assert_eq!(binding.matches(HELPER_INSTRUCTIONS).count(), 1);
    profiles
        .update(
            "specialist",
            profile("specialist", MODEL, Some("Edited")),
            None,
        )
        .await
        .unwrap();
    assert_eq!(instructions(&control, target).await, binding);
}

#[path = "conversation/retrieval.rs"]
mod retrieval;

#[path = "conversation/dreaming.rs"]
mod dreaming;

#[path = "conversation/deletion.rs"]
mod deletion;

#[path = "conversation/generation.rs"]
mod generation;

#[path = "conversation/workspaces.rs"]
mod workspaces;

const NO_STEER_CLIENT: pontia_agent_control::client_contract::AgentClientSpec =
    pontia_agent_control::client_contract::AgentClientSpec {
        client_type: "no-steer",
        capabilities: AgentClientCapabilities {
            steer: false,
            ..CAPABILITIES
        },
        ..BOT_CLIENT
    };

async fn restart_bot(
    control: &AgentControl,
    conversation: &ConversationService,
    bot: &str,
) -> String {
    let current = conversation
        .records()
        .get(bot)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    let runtime = control
        .queries()
        .session_snapshot(&current)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    control
        .session_commands()
        .terminate_session(&current)
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_client_fact(
            &current,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason":"test client exited"}),
        )
        .await
        .unwrap();
    conversation.send(bot, "continue").await.unwrap();
    let next = conversation
        .records()
        .get(bot)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    assert_ne!(next, current);
    next
}

fn listed_profiles(handoff: &str) -> Vec<&str> {
    handoff
        .split("## Agent Profiles\n")
        .nth(1)
        .unwrap()
        .split("## Helpers\n")
        .next()
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("- `"))
        .map(|line| line.strip_suffix('`').unwrap())
        .collect()
}

#[tokio::test]
async fn bot_handoff_lists_only_profiles_that_can_start_a_helper_at_each_launch() {
    let (_root, pool, _client, control, conversation) =
        fixture_with_clients(&[&BOT_CLIENT, &NO_STEER_CLIENT]).await;
    let (id, session) = bot(&conversation).await;
    assert!(listed_profiles(&instructions(&control, &session).await).is_empty());
    let profiles = control.agent_profiles();
    profiles
        .create(profile("reviewer", MODEL, Some("Review only")), None)
        .await
        .unwrap();
    profiles
        .create(profile("writer", MODEL, None), None)
        .await
        .unwrap();
    let mut no_steer = profile("no-steer", MODEL, None);
    no_steer.client_type = Some("no-steer".into());
    profiles.create(no_steer, None).await.unwrap();
    profiles
        .create(
            UpsertAgentProfileRequest {
                client_type: None,
                client_key: None,
                model: None,
                ..profile("no-defaults", MODEL, Some("Prompt only"))
            },
            None,
        )
        .await
        .unwrap();
    let session = restart_bot(&control, &conversation, &id).await;
    let handoff = instructions(&control, &session).await;
    assert_eq!(listed_profiles(&handoff), ["reviewer", "writer"]);
    let list = conversation
        .profile_command(
            serde_json::from_value(json!({
                "session_id":session,"command":{"action":"list"}
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    let summaries = list["profiles"].as_array().unwrap();
    assert_eq!(summaries.len(), 4);
    for summary in summaries {
        let id = summary["profile_id"].as_str().unwrap();
        assert_eq!(
            summary["available"],
            listed_profiles(&handoff).contains(&id)
        );
        assert_eq!(
            summary["unavailable_reason"].is_string(),
            !listed_profiles(&handoff).contains(&id)
        );
    }
    assert!(!handoff.contains("Review only"));

    let helpers = HelperService::new(control.clone(), conversation.clone());
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(
        create_helper(&helpers, &session, Some("no-steer")).await,
        Err(pontia_bot::helpers::HelperError::Control(
            Error::Unsupported(_)
        ))
    ));
    assert!(matches!(
        create_helper(&helpers, &session, Some("no-defaults")).await,
        Err(pontia_bot::helpers::HelperError::Control(Error::Domain(_)))
    ));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
    let writer = create_helper(&helpers, &session, Some("writer"))
        .await
        .unwrap();
    assert_eq!(
        instructions(&control, writer["session_id"].as_str().unwrap()).await,
        HELPER_INSTRUCTIONS
    );

    profiles.delete("writer", None).await.unwrap();
    profiles
        .create(profile("auditor", MODEL, None), None)
        .await
        .unwrap();
    assert_eq!(
        listed_profiles(&instructions(&control, &session).await),
        ["reviewer", "writer"]
    );
    let session = restart_bot(&control, &conversation, &id).await;
    assert_eq!(
        listed_profiles(&instructions(&control, &session).await),
        ["auditor", "reviewer"]
    );
}

#[tokio::test]
async fn profile_selected_from_a_handoff_launches_with_its_current_configuration() {
    let (_root, pool, client, control, conversation) = fixture().await;
    let profiles = control.agent_profiles();
    profiles
        .create(profile("specialist", "missing/model", None), None)
        .await
        .unwrap();
    let (_, session) = bot(&conversation).await;
    assert_eq!(
        listed_profiles(&instructions(&control, &session).await),
        ["specialist"]
    );
    profiles
        .update(
            "specialist",
            profile("specialist", MODEL, Some("Specialist prompt")),
            None,
        )
        .await
        .unwrap();
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let response = create_helper(&helpers, &session, Some("specialist"))
        .await
        .unwrap();
    let helper = response["session_id"].as_str().unwrap();
    assert_eq!(launched_model(&client, helper), Some(MODEL.into()));
    assert_eq!(
        instructions(&control, helper)
            .await
            .matches("Specialist prompt")
            .count(),
        1
    );
    profiles.delete("specialist", None).await.unwrap();
    profiles
        .create(profile("disabled", MODEL, None), None)
        .await
        .unwrap();
    AgentClientService::new(control.db(), control.clients())
        .save("generic", "default", false)
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(matches!(
        create_helper(&helpers, &session, Some("specialist")).await,
        Err(pontia_bot::helpers::HelperError::Control(Error::NotFound(
            _
        )))
    ));
    assert!(matches!(
        create_helper(&helpers, &session, Some("disabled")).await,
        Err(pontia_bot::helpers::HelperError::Control(Error::Conflict {
            code: "client_disabled",
            ..
        }))
    ));
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn helper_rejects_missing_and_deleted_profiles_before_creating_a_session() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (_, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    let profiles = control.agent_profiles();
    profiles
        .create(profile("deleted", MODEL, None), None)
        .await
        .unwrap();
    profiles.delete("deleted", None).await.unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    for profile_id in ["missing", "deleted"] {
        assert!(matches!(
            create_helper(&helpers, &session, Some(profile_id)).await,
            Err(pontia_bot::helpers::HelperError::Control(Error::NotFound(
                _
            )))
        ));
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(after, before);
}

#[path = "conversation/status.rs"]
mod status;

#[path = "conversation/compaction.rs"]
mod compaction;

#[path = "conversation/links.rs"]
mod links;

#[path = "conversation/quotes.rs"]
mod quotes;

#[path = "conversation/todos.rs"]
mod todos;
