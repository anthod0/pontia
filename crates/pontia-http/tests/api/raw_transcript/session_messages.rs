use super::*;
use pontia_bot::identity::{BotAccess, BotIdentity, BotRole, IdentityError};
use std::{future::Future, pin::Pin};

struct Fixture {
    root: tempfile::TempDir,
    state: AppState,
    transcript: PathBuf,
}

async fn fixture() -> Fixture {
    let root = tempdir().unwrap();
    let state = test_state().await;
    seed_session(&state, "sess_messages").await;
    let transcript = root.path().join("history.jsonl");
    let mut entries = [
        json!({"type":"session", "version":3, "id":"native-messages"}),
        json!({"type":"message", "id":"u1", "parentId":null, "message":{"role":"user", "content":"first question"}}),
        json!({"type":"message", "id":"a1", "parentId":"u1", "message":{"role":"assistant", "content":[{"type":"text", "text":"first answer"}]}}),
        json!({"type":"message", "id":"call", "parentId":"a1", "message":{"role":"assistant", "content":[{"type":"toolCall", "id":"read1", "name":"read", "arguments":{"path":"file.txt"}}]}}),
        json!({"type":"message", "id":"result", "parentId":"call", "message":{"role":"toolResult", "toolCallId":"read1", "toolName":"read", "content":[{"type":"text", "text":"file contents"}], "isError":false}}),
        json!({"type":"message", "id":"u3", "parentId":"result", "message":{"role":"user", "content":"latest question"}}),
        json!({"type":"message", "id":"a3", "parentId":"u3", "message":{"role":"assistant", "content":[{"type":"text", "text":"latest answer"}]}}),
    ];
    for entry in &mut entries {
        entry["timestamp"] = json!("2026-11-18T00:00:00Z");
    }
    fs::write(
        &transcript,
        entries
            .iter()
            .map(|entry| format!("{entry}\n"))
            .collect::<String>(),
    )
    .unwrap();
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: "sess_messages".into(),
            client_type: "pi".into(),
            launch_cwd: root.path().display().to_string(),
            client_session_key: "native-messages".into(),
            client_session_file: Some(transcript.display().to_string()),
            metadata: json!({}),
        })
        .await
        .unwrap();
    for (turn, head, tail) in [
        ("turn_1", None, "a1"),
        ("turn_2", Some("a1"), "result"),
        ("turn_3", Some("result"), "a3"),
    ] {
        post_pi_turn_event(
            state.clone(),
            "sess_messages",
            turn,
            "unused",
            "turn.started",
            json!({"previous_leaf_id":head}),
        )
        .await;
        post_pi_turn_event(
            state.clone(),
            "sess_messages",
            turn,
            "unused",
            "turn.completed",
            json!({"terminal_leaf_id":tail}),
        )
        .await;
    }
    sqlx::query("INSERT INTO inbox_messages (message_id,session_id,delivery_policy,input_summary,state) VALUES ('pending','sess_messages','after_idle','follow up','pending')")
        .execute(&state.db()).await.unwrap();
    Fixture {
        root,
        state,
        transcript,
    }
}

async fn post(state: impl Into<http::HttpState>, path: &str, input: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn read(state: &AppState, query: Value) -> Value {
    let (status, body) = post(state.clone(), "/api/v1/sessions/messages/read", query).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

#[tokio::test]
async fn recent_pages_continue_through_filtered_turns_and_preserve_execution_state() {
    let f = fixture().await;
    let (status, before) = get_json(f.state.clone(), "/api/v1/sessions/sess_messages").await;
    assert_eq!(status, StatusCode::OK, "{before}");
    let recent = read(&f.state, json!({"session_id":"sess_messages", "limit":1})).await;
    assert_eq!(recent["items"][0]["content_preview"], "latest question");
    assert_eq!(recent["items"][1]["item_id"], "pi:entry:a3:block:0");
    assert_eq!(recent["next"]["turn_id"], "turn_2");
    assert_eq!(
        recent["next"]["categories"],
        json!(["user", "assistant", "other"])
    );
    assert_eq!(recent["has_more"], true);
    let middle = read(&f.state, recent["next"].clone()).await;
    assert_eq!(middle["items"], json!([]));
    assert_eq!(middle["has_more"], true);
    assert_eq!(middle["next"]["turn_id"], "turn_1");
    let oldest = read(&f.state, middle["next"].clone()).await;
    assert_eq!(oldest["items"][0]["content_preview"], "first question");
    assert_eq!(oldest["items"].as_array().unwrap().len(), 2);
    assert_eq!(oldest["has_more"], false);
    assert!(oldest["next"].is_null());
    let (status, after) = get_json(f.state.clone(), "/api/v1/sessions/sess_messages").await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(before, after);
    let inbox: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT message_id,input_summary,state FROM inbox_messages WHERE session_id='sess_messages'",
    )
    .fetch_all(&f.state.db())
    .await
    .unwrap();
    assert_eq!(
        inbox,
        vec![("pending".into(), "follow up".into(), "pending".into())]
    );
}

#[tokio::test]
async fn anchored_fragments_include_explicit_categories_and_page_forward() {
    let f = fixture().await;
    let fragment = read(&f.state, json!({"session_id":"sess_messages", "turn_id":"turn_2", "direction":"forward", "limit":1, "categories":["tool_call","tool_result"]})).await;
    let items = fragment["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["kind"], "tool_call");
    assert_eq!(items[1]["kind"], "tool_result");
    assert_eq!(items[0]["call_id"], items[1]["call_id"]);
    assert_eq!(items[1]["content_preview"], "file contents");
    assert_eq!(
        fragment["next"]["categories"],
        json!(["tool_call", "tool_result"])
    );
    let last = read(&f.state, fragment["next"].clone()).await;
    assert_eq!(last["items"], json!([]));
    assert_eq!(last["has_more"], false);
    let users = read(
        &f.state,
        json!({"session_id":"sess_messages", "categories":["user"]}),
    )
    .await;
    assert_eq!(users["items"].as_array().unwrap().len(), 2);
    assert!(
        users["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["role"] == "user")
    );
}

#[tokio::test]
async fn missing_failed_sources_and_invalid_queries_do_not_look_like_empty_history() {
    let f = fixture().await;
    for query in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"categories":[]}),
        json!({"categories":["unknown"]}),
        json!({"direction":"sideways"}),
    ] {
        let mut input = query;
        input["session_id"] = json!("sess_messages");
        let (status, _) = post(f.state.clone(), "/api/v1/sessions/messages/read", input).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    for (session, anchor, code) in [
        ("missing", None, "session_not_found"),
        ("sess_messages", Some("missing"), "turn_not_found"),
    ] {
        let (_, body) = post(
            f.state.clone(),
            "/api/v1/sessions/messages/read",
            json!({"session_id":session, "turn_id":anchor}),
        )
        .await;
        assert_eq!(body["error"]["code"], code);
    }
    seed_session(&f.state, "unbound").await;
    let (status, body) = post(
        f.state.clone(),
        "/api/v1/sessions/messages/read",
        json!({"session_id":"unbound"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "timeline_source_unavailable");
    fs::rename(&f.transcript, f.root.path().join("moved.jsonl")).unwrap();
    let (status, body) = post(
        f.state.clone(),
        "/api/v1/sessions/messages/read",
        json!({"session_id":"sess_messages"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "timeline_source_unavailable");
    fs::write(&f.transcript, "not JSON\n").unwrap();
    let (status, body) = post(
        f.state.clone(),
        "/api/v1/sessions/messages/read",
        json!({"session_id":"sess_messages"}),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body["data"].is_null());
}

struct Identity(BotRole);
impl BotIdentity for Identity {
    fn resolve<'a>(
        &'a self,
        session: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<BotAccess, IdentityError>> + Send + 'a>> {
        Box::pin(async move {
            if session != "caller" {
                return Err(IdentityError);
            }
            Ok(BotAccess {
                role: self.0,
                workspace_id: None,
            })
        })
    }
}

#[tokio::test]
async fn bot_readers_share_defaults_and_require_authorized_identity() {
    let f = fixture().await;
    let input = json!({"session_id":"caller", "input":{"session_id":"sess_messages", "limit":1}});
    let expected = read(&f.state, input["input"].clone()).await;
    for role in [
        BotRole::Bot,
        BotRole::Retrieval,
        BotRole::Dreaming,
        BotRole::Helper,
    ] {
        let state =
            http::HttpState::from(f.state.clone()).with_bot_identity(Arc::new(Identity(role)));
        let (status, body) = post(state, "/api/v1/bot/session-messages", input.clone()).await;
        if role == BotRole::Helper {
            assert_eq!(status, StatusCode::FORBIDDEN);
        } else {
            assert_eq!(status, StatusCode::OK, "{body}");
            assert_eq!(body["data"], expected);
        }
    }
    let (status, _) = post(f.state, "/api/v1/bot/session-messages", input).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reading_active_runtime_history_leaves_turn_and_pending_input_unchanged() {
    let f = fixture().await;
    post_pi_turn_event(
        f.state.clone(),
        "sess_messages",
        "turn_4",
        "unused",
        "turn.started",
        json!({"previous_leaf_id":"a3"}),
    )
    .await;
    fs::OpenOptions::new().append(true).open(&f.transcript).unwrap()
        .write_all(format!("{}\n", json!({"type":"message", "id":"a4", "parentId":"a3", "timestamp":"2026-11-18T00:00:01Z", "message":{"role":"assistant", "content":[{"type":"text", "text":"still working"}]}})).as_bytes()).unwrap();
    let peer = super::runtime_history::attach_fixture(
        &f.state,
        "sess_messages",
        "native-messages",
        f.transcript.clone(),
    )
    .await;
    let service = f.state.control().event_ingest_service();
    let before = service.get_turn("turn_4").await.unwrap().unwrap();
    let page = read(&f.state, json!({"session_id":"sess_messages", "limit":1})).await;
    assert_eq!(page["items"][0]["content_preview"], "still working");
    let after = service.get_turn("turn_4").await.unwrap().unwrap();
    assert_eq!(before.state, after.state);
    assert_eq!(before.state_version, after.state_version);
    assert!(after.tail_cursor.is_none());
    let pending: (String, Option<String>) =
        sqlx::query_as("SELECT state,turn_id FROM inbox_messages WHERE message_id='pending'")
            .fetch_one(&f.state.db())
            .await
            .unwrap();
    assert_eq!(pending, ("pending".into(), None));
    peer.close();
}

#[tokio::test]
async fn helper_check_reads_paginated_native_history_without_changing_execution() {
    let f = fixture().await;
    seed_session(&f.state, "caller").await;
    let pool = f.state.db();
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id) VALUES ('bot-check','Bot','pi','default','test','caller')").execute(&pool).await.unwrap();
    sqlx::query("UPDATE sessions SET application_id='bot', application_ref='bot-check:launch', runtime_environment=? WHERE session_id='caller'")
        .bind(json!({"PONTIA_BOT_TOOL_TOKEN":"caller-credential"}).to_string()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:check','bot-check','sess_messages')").execute(&pool).await.unwrap();
    let state = http::HttpState::from(f.state.clone());
    let before = f
        .state
        .control()
        .queries()
        .session_snapshot("sess_messages")
        .await
        .unwrap()
        .unwrap();
    let request =
        |input: Value| json!({"session_id":"caller", "command":{"action":"check", "input":input}});
    let input = request(json!({"session_id":"sess_messages", "limit":1}));
    let (status, _) = post(state.clone(), "/api/v1/bot/helper", input.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let send = |input: Value| {
        let state = state.clone();
        async move {
            let response = http::router(state)
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/v1/bot/helper")
                        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("x-pontia-bot-tool-token", "caller-credential")
                        .body(Body::from(input.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let body: Value =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(status, StatusCode::OK, "{body}");
            body["data"].clone()
        }
    };
    let recent = send(input).await;
    assert_eq!(recent["session"]["state"], before.state);
    assert_eq!(recent["messages"]["items"].as_array().unwrap().len(), 2);
    let middle = send(request(recent["messages"]["next"].clone())).await;
    assert_eq!(middle["messages"]["items"], json!([]));
    assert_eq!(middle["messages"]["has_more"], true);
    let details = send(request(json!({"session_id":"sess_messages", "turn_id":"turn_2", "limit":1, "categories":["tool_call","tool_result"]}))).await;
    assert_eq!(details["messages"]["items"].as_array().unwrap().len(), 2);
    let after = f
        .state
        .control()
        .queries()
        .session_snapshot("sess_messages")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.event_position, after.event_position);
    assert_eq!(before.current_turn_id, after.current_turn_id);
    assert_eq!(before.inbox.len(), after.inbox.len());
}

#[tokio::test]
async fn startup_failure_can_be_checked_without_history_and_closed_without_a_control_channel() {
    use pontia_agent_control::{PontiaEvent, PontiaEventSource, PontiaEventType};
    let state = test_state().await;
    seed_session(&state, "caller").await;
    seed_session(&state, "failed-helper").await;
    let pool = state.db();
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id) VALUES ('bot-check','Bot','pi','default','test','caller')").execute(&pool).await.unwrap();
    sqlx::query("UPDATE sessions SET application_id='bot', application_ref='bot-check:launch' WHERE session_id='caller'").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:check','bot-check','failed-helper')").execute(&pool).await.unwrap();
    state.control().event_ingest_service().ingest_pontia_event(PontiaEvent::new(
        "failed-helper", None, PontiaEventSource::RuntimeManager, "pi", PontiaEventType::SessionError,
        json!({"runtime_id":"runtime_failed-helper", "reason":"startup_failed", "failure":{"message":"launch failed"}})
    )).await.unwrap();
    let http_state =
        http::HttpState::from(state.clone()).with_bot_identity(Arc::new(Identity(BotRole::Bot)));
    let (status, body) = post(http_state.clone(), "/api/v1/bot/helper", json!({"session_id":"caller", "command":{"action":"check","input":{"session_id":"failed-helper"}}})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["session"]["state"], "error");
    assert!(body["data"]["messages"].is_null());
    assert!(body["data"]["messages_unavailable"].is_string());
    let (status, body) = post(http_state.clone(), "/api/v1/bot/helper", json!({"session_id":"caller", "command":{"action":"check","input":{"session_id":"failed-helper", "limit":0}}})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body) = post(http_state, "/api/v1/bot/helper", json!({"session_id":"caller", "command":{"action":"close","input":{"session_id":"failed-helper"}}})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["session"]["state"], "exited");
    let events = state
        .control()
        .queries()
        .list_session_events("failed-helper")
        .await
        .unwrap();
    let exited = events
        .iter()
        .find(|event| event.event_type == "session.exited")
        .unwrap();
    assert_eq!(exited.source, "external_api");
    assert_eq!(exited.payload["reason"], "management_exit");
}

#[tokio::test]
async fn retrieval_profile_reads_arbitrary_sessions_without_memory_associations() {
    let f = fixture().await;
    seed_session(&f.state, "retrieval-caller").await;
    let pool = f.state.db();
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model) VALUES ('reader-bot','Bot','pi','default','test')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:reader','reader-bot','retrieval-caller')").execute(&pool).await.unwrap();
    sqlx::query("UPDATE sessions SET execution_profile_id='reader-bot-retrieval', runtime_environment=? WHERE session_id='retrieval-caller'")
        .bind(json!({"PONTIA_BOT_TOOL_TOKEN":"reader-credential"}).to_string()).execute(&pool).await.unwrap();
    let request =
        json!({"session_id":"retrieval-caller", "input":{"session_id":"sess_messages", "limit":1}});
    let expected = read(&f.state, request["input"].clone()).await;
    let response = http::router(http::HttpState::from(f.state.clone()))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bot/session-messages")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-pontia-bot-tool-token", "reader-credential")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["data"], expected);
    let memories: i64 = sqlx::query_scalar("SELECT count(*) FROM bot_memories")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(memories, 0);
}
