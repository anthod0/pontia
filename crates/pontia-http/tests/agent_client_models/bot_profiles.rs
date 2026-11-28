use super::*;
use sha2::Digest;

async fn profile_tool(
    app: &Router,
    session: &str,
    token: &str,
    command: Value,
) -> (StatusCode, Value) {
    let body = json!({"session_id":session, "command":command});
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/bot/profile")
                .header("Authorization", "Bearer test-token")
                .header(pontia_bot::identity::TOOL_TOKEN_HEADER, token)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn bot_fixture() -> (Router, sqlx::SqlitePool, tempfile::TempDir, EnvGuard) {
    let (app, db, root, _env) = fixture(json!({"success":true,"data":{"models":[
        {"provider":"p","id":"one","name":"One"}, {"provider":"p","id":"two","name":"Two"}
    ]}}))
    .await;
    for (session, owner) in [
        ("current", "bot-a:current"),
        ("old", "bot-a:old"),
        ("helper", "helper:bot-a:1"),
        ("retrieval", "helper:bot-a:2"),
        ("dream", "dreaming:bot-a"),
    ] {
        sqlx::query("INSERT INTO sessions(session_id,client_type,state,application_id,application_ref,credential_env, credential_sha256) VALUES (?, 'pi', 'idle', 'bot', ?, 'PONTIA_BOT_TOOL_TOKEN', ?)")
            .bind(session).bind(owner).bind(sha2::Sha256::digest(session).to_vec())
            .execute(&db).await.unwrap();
    }
    sqlx::query("INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id) VALUES ('bot-a','Bot','pi','default','p/one','current')")
        .execute(&db).await.unwrap();
    (app, db, root, _env)
}

#[tokio::test]
async fn only_the_current_bot_can_read_or_change_profiles() {
    let (app, _, _root, _env) = bot_fixture().await;
    for (session, token) in [
        ("current", "wrong"),
        ("old", "old"),
        ("helper", "helper"),
        ("retrieval", "retrieval"),
        ("dream", "dream"),
    ] {
        for command in [
            json!({"action":"list"}),
            json!({"action":"delete", "input":{"profile_id":"reviewer"}}),
        ] {
            assert_eq!(
                profile_tool(&app, session, token, command).await.0,
                StatusCode::FORBIDDEN
            );
        }
    }
}

#[tokio::test]
async fn bot_profile_crud_uses_catalog_models_and_preserves_omitted_fields() {
    let (app, db, _root, _env) = bot_fixture().await;
    let target = json!({"profile_id":"reviewer"});
    let client = json!({"client_type":"pi","client_key":"default"});
    let input = json!({"profile_id":"reviewer","name":"Reviewer","description":"Review changes","client":client,"model":"p/one","system_prompt":"Review carefully"});
    let create = json!({"action":"create","input":input});
    let call = |command| profile_tool(&app, "current", "current", command);
    let (status, clients) = call(json!({"action":"clients"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(clients["data"]["clients"].as_array().unwrap().len(), 1);
    assert_eq!(clients["data"]["clients"][0]["client"], "pi/default");
    let (_, catalog) = call(json!({"action":"models","input":{"client":client}})).await;
    assert_eq!(catalog["data"]["models"][0]["id"], "p/one");
    for (key, value) in [
        ("description", json!(" ")),
        ("description", Value::Null),
        ("profile_id", json!("invalid ID")),
        ("model", json!("missing")),
    ] {
        let mut invalid = create.clone();
        invalid["input"][key] = value;
        assert_eq!(call(invalid).await.0, StatusCode::BAD_REQUEST);
    }
    let (status, created) = call(create.clone()).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(call(create).await.0, StatusCode::CONFLICT);
    let (_, shown) = call(json!({"action":"show","input":target})).await;
    assert_eq!(shown["data"], created["data"]);
    let (_, list) = call(json!({"action":"list"})).await;
    let summary = &list["data"]["profiles"][0];
    assert_eq!(summary["available"], true);
    assert_eq!(summary["has_system_prompt"], true);
    assert!(summary.get("system_prompt").is_none());
    let patch = |fields: Value| {
        let mut input = target.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        call(json!({"action":"update","input":input}))
    };
    for fields in [
        json!({}),
        json!({"client":client}),
        json!({"model":"missing"}),
        json!({"description":"a","clear_description":true}),
    ] {
        assert_eq!(patch(fields).await.0, StatusCode::BAD_REQUEST);
    }
    let (status, changed) = patch(json!({"model":"p/two"})).await;
    assert_eq!(status, StatusCode::OK);
    let changed = &changed["data"]["profile"];
    assert_eq!(changed["model"], "p/two");
    for field in [
        "name",
        "description",
        "system_prompt",
        "client_type",
        "client_key",
    ] {
        assert_eq!(changed[field], created["data"]["profile"][field]);
    }
    let (status, changed) =
        patch(json!({"name":"Editor","clear_description":true,"clear_system_prompt":true})).await;
    assert_eq!(status, StatusCode::OK);
    let changed = &changed["data"]["profile"];
    assert_eq!(changed["name"], "Editor");
    assert_eq!(changed["description"], Value::Null);
    assert_eq!(changed["system_prompt"], Value::Null);
    sqlx::query("UPDATE agent_clients SET enabled=0 WHERE client_type='pi'")
        .execute(&db)
        .await
        .unwrap();
    let (_, list) = call(json!({"action":"list"})).await;
    let summary = &list["data"]["profiles"][0];
    assert_eq!(summary["available"], false);
    assert!(
        summary["unavailable_reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert_eq!(
        call(json!({"action":"delete","input":target})).await.0,
        StatusCode::OK
    );
    for action in ["show", "delete"] {
        assert_eq!(
            call(json!({"action":action,"input":target})).await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(patch(json!({"name":"x"})).await.0, StatusCode::NOT_FOUND);
}
