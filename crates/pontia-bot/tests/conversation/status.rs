use super::*;

struct UnavailableStatusClient;

impl InProcessClient for UnavailableStatusClient {
    fn capabilities(&self) -> AgentClientCapabilities {
        panic!("Work status must not probe a client")
    }

    fn ready(&self, _: &str, _: &str) -> ReportedEvent {
        unreachable!()
    }

    fn submit(&self, _: AgentInput) -> Result<()> {
        unreachable!()
    }
}

#[tokio::test]
async fn chat_and_timeline_keep_helper_status_without_client_probes() {
    let (root, pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let older = helper(&helpers, &session).await;
    let newer = helper(&helpers, &session).await;
    start(&control, &older).await;
    start(&control, &newer).await;
    answer(&conversation, &session).await;
    sqlx::query(
        "UPDATE sessions SET created_at='2026-01-01',title='Task' WHERE session_id IN (?,?)",
    )
    .bind(&older)
    .bind(&newer)
    .execute(&pool)
    .await
    .unwrap();
    let latest = [&older, &newer].into_iter().max().unwrap();
    sqlx::query("UPDATE sessions SET title='Latest task' WHERE session_id=?")
        .bind(latest)
        .execute(&pool)
        .await
        .unwrap();
    let expected = conversation.chat(&id).await.unwrap().status;
    assert_eq!(
        expected,
        WorkStatus::Working {
            title: "Latest task".into()
        }
    );

    let mut registration = test_registration();
    registration.in_process = Some(Arc::new(UnavailableStatusClient));
    let mut registry = ClientRegistry::default();
    registry.register(registration);
    let read_control = AgentControl::builder(pool.clone(), root.path().into())
        .clients(registry)
        .build();
    let reader = test_conversation(read_control);
    assert_eq!(reader.chat(&id).await.unwrap().status, expected);
    let page = serde_json::to_value(reader.timeline(&id, None, None, 50).await.unwrap()).unwrap();
    assert_eq!(page["status"], serde_json::to_value(expected).unwrap());

    // A missing title on the newest helper keeps the existing Idle result.
    sqlx::query("UPDATE sessions SET title=NULL WHERE session_id=?")
        .bind(latest)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(reader.chat(&id).await.unwrap().status, WorkStatus::Idle);
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id=?")
        .bind(latest)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        reader.chat(&id).await.unwrap().status,
        WorkStatus::Working {
            title: "Task".into()
        }
    );
}

async fn answer(conversation: &ConversationService, session: &str) {
    conversation
        .send_to_user(ReplyRequest {
            session_id: session.into(),
            input: Reply {
                todo: false,
                resolve_todos: vec![],
                related_to: vec![],
                message: "Received".into(),
            },
        })
        .await
        .unwrap();
}

pub(super) async fn start(control: &AgentControl, session: &str) {
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
}

#[tokio::test]
async fn newest_busy_helper_follows_processing_and_retrieval_and_dreaming_do_not_count() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let older = helper(&helpers, &session).await;
    let newer = helper(&helpers, &session).await;
    start(&control, &older).await;
    start(&control, &newer).await;
    for (helper, title, date) in [
        (&older, "Older task", "2026-01-01T00:00:00Z"),
        (&newer, "Newer task", "2026-01-02T00:00:00Z"),
    ] {
        sqlx::query("UPDATE sessions SET title=?,created_at=? WHERE session_id=?")
            .bind(title)
            .bind(date)
            .bind(helper)
            .execute(&pool)
            .await
            .unwrap();
    }
    let retrieval = helpers
        .execute(HelperRequest {
            session_id: session.clone(),
            command: HelperCommand::Retrieve(pontia_bot::helpers::RetrieveMemories {
                message: "Find memories".into(),
                workspace_id: None,
            }),
        })
        .await
        .unwrap();
    start(&control, retrieval["session_id"].as_str().unwrap()).await;
    // Dreaming has device ownership and is never a Bot helper.
    sqlx::query("INSERT INTO sessions(session_id,client_type,state,title) VALUES ('dreaming','generic','busy','Dreaming')").execute(&pool).await.unwrap();
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    answer(&conversation, &session).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Working {
            title: "Newer task".into()
        }
    );
    assert_eq!(
        test_conversation(control.clone())
            .chat(&id)
            .await
            .unwrap()
            .status,
        WorkStatus::Working {
            title: "Newer task".into()
        }
    );
    finish(&control, &newer, EventType::TurnCompleted).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Working {
            title: "Older task".into()
        }
    );
    finish(&control, &older, EventType::TurnCompleted).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Idle
    );
}

#[tokio::test]
async fn helper_replies_and_system_notifications_process_until_answer_or_turn_end() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &session).await;
    finish(&control, &worker, EventType::TurnCompleted).await;
    answer(&conversation, &session).await;
    let mut updates = conversation.subscribe(&id);
    helpers
        .execute(HelperRequest {
            session_id: worker,
            command: HelperCommand::Reply {
                message: "Ready for review".into(),
            },
        })
        .await
        .unwrap();
    assert!(matches!(
        updates.recv().await.unwrap(),
        ConversationUpdate::Changed
    ));
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    assert_eq!(
        test_conversation(control.clone())
            .chat(&id)
            .await
            .unwrap()
            .status,
        WorkStatus::Processing
    );
    finish(&control, &session, EventType::TurnCompleted).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Idle
    );
    conversation
        .deliver_input(&id, "Source: System Notification\nWorkspace registered")
        .await
        .unwrap();
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    answer(&conversation, &session).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Idle
    );
    conversation
        .deliver_input(&id, "Source: System Notification\nAnother notification")
        .await
        .unwrap();
    finish(&control, &session, EventType::TurnFailed).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Idle
    );
}

#[tokio::test]
async fn answered_bot_stays_busy_until_its_own_turn_ends_unless_a_helper_is_working() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (id, session) = bot(&conversation).await;
    start(&control, &session).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    answer(&conversation, &session).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Busy
    );
    let page =
        serde_json::to_value(conversation.timeline(&id, None, None, 50).await.unwrap()).unwrap();
    assert_eq!(page["status"], json!({"kind":"busy"}));

    let helpers = HelperService::new(control.clone(), conversation.clone());
    let worker = helper(&helpers, &session).await;
    start(&control, &worker).await;
    sqlx::query("UPDATE sessions SET title='Task' WHERE session_id=?")
        .bind(&worker)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Working {
            title: "Task".into()
        }
    );
    finish(&control, &worker, EventType::TurnCompleted).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Busy
    );

    finish(&control, &session, EventType::TurnCompleted).await;
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Idle
    );
}

#[tokio::test]
async fn helper_creation_rejects_missing_or_blank_titles_without_creating_sessions() {
    let (_root, pool, _client, control, conversation) = fixture().await;
    let (_, session) = bot(&conversation).await;
    let helpers = HelperService::new(control, conversation);
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut request = json!({"session_id":session,"command":{"action":"create","input":{"temporary":"task","description":"Task","message":"Start"}}});
    assert!(serde_json::from_value::<HelperRequest>(request.clone()).is_err());
    for title in ["", "  "] {
        request["command"]["input"]["title"] = json!(title);
        let error = helpers
            .execute(serde_json::from_value(request.clone()).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            pontia_bot::helpers::HelperError::Control(Error::Domain(_))
        ));
    }
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn accepted_input_stays_processing_before_the_native_turn_arrives_and_survives_restart() {
    use pontia_agent_control::client_contract::{
        AgentClientAdapter, AgentClientSpec, TurnLifecycleBehavior,
    };
    const NATIVE_TURNS: AgentClientSpec = AgentClientSpec {
        adapter: AgentClientAdapter {
            turn_lifecycle: TurnLifecycleBehavior::ClientManagedInteractive,
            ..BOT_CLIENT.adapter
        },
        ..BOT_CLIENT
    };
    let (_root, _pool, _client, control, conversation) = fixture_with_client(&NATIVE_TURNS).await;
    let (id, session) = bot(&conversation).await;
    assert!(
        !control
            .queries()
            .session_has_active_turn(&session)
            .await
            .unwrap()
    );
    assert_eq!(
        conversation.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    let recovered = test_conversation(control.clone());
    assert_eq!(
        recovered.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    let snapshot = control
        .queries()
        .session_snapshot(&session)
        .await
        .unwrap()
        .unwrap();
    control
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: session.clone(),
            turn_id: None,
            report_key: Some("delayed-start".into()),
            fact_type: EventType::TurnStarted,
            data: json!({"runtime_id":snapshot.runtimes[0].runtime_id}),
        })
        .await
        .unwrap();
    finish(&control, &session, EventType::TurnCompleted).await;
    assert_eq!(recovered.chat(&id).await.unwrap().status, WorkStatus::Idle);
    recovered
        .deliver_input(&id, "Source: Helper\n\nContinue")
        .await
        .unwrap();
    assert_eq!(
        recovered.chat(&id).await.unwrap().status,
        WorkStatus::Processing
    );
    answer(&recovered, &session).await;
    assert_eq!(recovered.chat(&id).await.unwrap().status, WorkStatus::Idle);
}
