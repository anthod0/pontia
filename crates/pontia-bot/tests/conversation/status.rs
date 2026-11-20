use super::*;

async fn answer(conversation: &ConversationService, session: &str) {
    conversation
        .send_to_user(ReplyRequest {
            session_id: session.into(),
            input: Reply {
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
        .deliver_input(&id, "Source: System\n\nWorkspace registered")
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
        .deliver_input(&id, "Source: System\n\nAnother notification")
        .await
        .unwrap();
    finish(&control, &session, EventType::TurnFailed).await;
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
