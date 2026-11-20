use std::sync::Arc;

use pontia_agent_control::{
    AgentProfileService, UpsertAgentProfileRequest,
    client_contract::{ClientConfiguration, ClientInstructions, test_registration},
    clients::ClientRegistry,
};
use pontia_bot::{
    messages::{MessageKind, MessageService},
    records::*,
};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};

struct Configuration;
impl ClientConfiguration for Configuration {
    fn keys(&self) -> Vec<String> {
        vec!["a".into(), "b".into()]
    }
    fn presentation(&self, key: &str, _: &Value) -> (String, String) {
        (key.into(), key.into())
    }
    fn key(&self, _: &Value) -> Result<String> {
        Ok("a".into())
    }
    fn merged(&self, _: &str, overrides: &Value) -> Value {
        overrides.clone()
    }
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        if !self.keys().iter().any(|known| known == key) {
            return Err(Error::Domain("Unknown client key".into()));
        }
        Ok(overrides.clone())
    }
}

struct Profile;
impl ClientInstructions for Profile {
    fn inject_standalone_session_instructions(&self) -> bool {
        false
    }
    fn validate_instructions(&self, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn bind(
        &self,
        input: &pontia_agent_control::client_contract::InstructionsBindingInput<'_>,
    ) -> Result<Value> {
        Ok(json!({"system": input.system_prompt}))
    }
}

struct Fixture {
    root: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    bots: BotService,
    messages: MessageService,
    profiles: AgentProfileService,
    registry: ClientRegistry,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!(
            "sqlite://{}",
            root.path().join("records.db").display()
        ))
        .await
        .unwrap();
        run_migrations(&pool).await.unwrap();
        let settings =
            pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
                pool.clone(),
            );
        for key in ["a", "b"] {
            settings
                .save("generic", key, Some(true), None)
                .await
                .unwrap();
        }

        let mut registry = ClientRegistry::default();
        let mut client = test_registration();
        client.configuration = Some(Arc::new(Configuration));
        client.instructions = Some(Arc::new(Profile));
        registry.register(client);
        Self {
            bots: BotService::new(pool.clone(), root.path(), registry.clone()),
            messages: MessageService::new(pool.clone()),
            profiles: AgentProfileService::new(pool.clone()).with_clients(registry.clone()),
            root,
            pool,
            registry,
        }
    }

    async fn profile_request(&self, id: &str) -> UpsertAgentProfileRequest {
        let profile = self.profiles.get(id).await.unwrap().unwrap();
        {
            let mut value = serde_json::to_value(profile).unwrap();
            value.as_object_mut().unwrap().remove("created_at");
            value.as_object_mut().unwrap().remove("updated_at");
            serde_json::from_value(value).unwrap()
        }
    }
}

fn configuration(name: &str, key: &str, model: &str) -> BotConfiguration {
    BotConfiguration {
        name: name.into(),
        description: Some(format!("{name} description")),
        avatar_shape: None,
        avatar_color: None,
        client_type: "generic".into(),
        client_key: key.into(),
        model: model.into(),
    }
}

#[tokio::test]
async fn avatar_selection_persists_across_updates_and_session_changes() {
    let f = Fixture::new().await;
    let mut input = configuration("A", "a", "model");
    input.avatar_shape = Some(AvatarShape::Mochi);
    input.avatar_color = Some(AvatarColor::Rose);
    let bot = f.bots.create(input.clone()).await.unwrap();
    assert_eq!(bot.avatar_shape, AvatarShape::Mochi);
    assert_eq!(bot.avatar_color, AvatarColor::Rose);
    input.avatar_shape = Some(AvatarShape::Onigiri);
    input.avatar_color = Some(AvatarColor::Indigo);
    f.bots.update(&bot.bot_id, input).await.unwrap();
    f.bots
        .update(&bot.bot_id, configuration("Renamed", "a", "model"))
        .await
        .unwrap();
    f.bots
        .set_current_session(&bot.bot_id, Some("next-generation"))
        .await
        .unwrap();
    let saved = f.bots.get(&bot.bot_id).await.unwrap();
    assert_eq!(saved.avatar_shape, AvatarShape::Onigiri);
    assert_eq!(saved.avatar_color, AvatarColor::Indigo);
    assert_eq!(f.bots.list().await.unwrap(), vec![saved]);
}

#[tokio::test]
async fn omitted_avatar_uses_creation_defaults() {
    let f = Fixture::new().await;
    let input = serde_json::from_value(json!({
        "name": "A", "description": null,
        "client_type": "generic", "client_key": "a", "model": "model"
    }))
    .unwrap();
    let bot = f.bots.create(input).await.unwrap();
    assert_eq!(bot.avatar_shape, AvatarShape::Classic);
    assert_eq!(bot.avatar_color, AvatarColor::Blue);
}

#[test]
fn bot_configuration_rejects_unknown_avatar_options() {
    for (field, value) in [("avatar_shape", "triangle"), ("avatar_color", "#458588")] {
        let mut input = serde_json::to_value(configuration("A", "a", "model")).unwrap();
        input[field] = json!(value);
        assert!(serde_json::from_value::<BotConfiguration>(input).is_err());
    }
}

#[tokio::test]
async fn bots_have_independent_configuration_directories_and_current_sessions() {
    let f = Fixture::new().await;
    let a = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap();
    let mut input = configuration("B", "b", "model-b");
    input.description = None;
    let b = f.bots.create(input).await.unwrap();
    assert_ne!(a.bot_id, b.bot_id);
    assert_eq!(b.description, None);
    assert_eq!(a.description.as_deref(), Some("A description"));
    assert_eq!((&a.client_key, &a.model), (&"a".into(), &"model-a".into()));
    assert_eq!((&b.client_key, &b.model), (&"b".into(), &"model-b".into()));
    assert!(a.current_session_id.is_none());
    let directory = f.bots.directory(&a.bot_id).await.unwrap();
    let settings: Value =
        serde_json::from_slice(&std::fs::read(directory.join(".pi/settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["steeringMode"], "all");
    assert!(f.bots.directory(&b.bot_id).await.unwrap().is_dir());
    assert_ne!(directory, f.bots.directory(&b.bot_id).await.unwrap());
    std::fs::write(directory.join("notes.txt"), "keep me").unwrap();
    f.bots
        .set_current_session(&a.bot_id, Some("opaque-session"))
        .await
        .unwrap();
    let renamed = f
        .bots
        .update(&a.bot_id, configuration("Renamed", "b", "model-new"))
        .await
        .unwrap();
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(
        renamed.current_session_id.as_deref(),
        Some("opaque-session")
    );
    assert_eq!(directory, f.bots.directory(&a.bot_id).await.unwrap());
    assert_eq!(
        std::fs::read_to_string(directory.join("notes.txt")).unwrap(),
        "keep me"
    );
    assert_eq!(f.bots.get(&b.bot_id).await.unwrap(), b);
    assert!(
        f.bots
            .set_current_session(&a.bot_id, None)
            .await
            .unwrap()
            .current_session_id
            .is_none()
    );
    assert_eq!(f.bots.list().await.unwrap().len(), 2);
}

#[tokio::test]
async fn defaults_capture_creation_config_and_preserve_user_edits_and_shared_resources() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.root.path().join("bot")).unwrap();
    let user = f.root.path().join("bot/USER.md");
    std::fs::write(&user, "existing user information").unwrap();
    let a = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap();
    let helper = BotService::helper_profile_id(&a.bot_id);
    let initial = f.profiles.resolve_launch(&helper).await.unwrap();
    assert_eq!(
        (
            initial.client_type.as_deref(),
            initial.client_key.as_deref(),
            initial.model.as_deref()
        ),
        (Some("generic"), Some("a"), Some("model-a"))
    );
    let mut request = f.profile_request(&helper).await;
    request.client_key = Some("b".into());
    request.model = Some("custom-model".into());
    request.system_prompt = Some("custom instructions".into());
    f.profiles.update(&helper, request).await.unwrap();
    let b = f
        .bots
        .create(configuration("B", "b", "model-b"))
        .await
        .unwrap();
    f.bots
        .update(&a.bot_id, configuration("Changed", "b", "model-new"))
        .await
        .unwrap();
    f.bots.initialize(&a.bot_id).await.unwrap();
    assert_eq!(
        f.profiles
            .resolve_launch(&BotService::helper_profile_id(&b.bot_id))
            .await
            .unwrap()
            .model
            .as_deref(),
        Some("model-b")
    );
    assert_eq!(
        f.profiles
            .resolve_launch(&helper)
            .await
            .unwrap()
            .model
            .as_deref(),
        Some("custom-model")
    );
    let launch = f.profiles.resolve_launch(&helper).await.unwrap();
    assert_eq!(launch.system_prompt.as_deref(), Some("custom instructions"));
    assert_eq!(
        std::fs::read_to_string(user).unwrap(),
        "existing user information"
    );
}

#[tokio::test]
async fn filesystem_initialization_failure_can_be_retried_with_the_saved_bot_id() {
    let f = Fixture::new().await;
    let user = f.root.path().join("bot/USER.md");
    std::fs::create_dir_all(&user).unwrap();
    let error = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap_err();
    let CreateError::Initialization {
        bot_id,
        source: Error::Io(_),
    } = error
    else {
        panic!("expected initialization IO failure")
    };
    assert_eq!(f.bots.list().await.unwrap().len(), 1);
    std::fs::remove_dir(&user).unwrap();
    f.bots.initialize(&bot_id).await.unwrap();
    assert_eq!(std::fs::read_to_string(&user).unwrap(), "");
    std::fs::write(&user, "user edited this").unwrap();
    let reopened = BotService::new(f.pool.clone(), f.root.path(), f.registry.clone());
    reopened.initialize(&bot_id).await.unwrap();
    assert_eq!(std::fs::read_to_string(user).unwrap(), "user edited this");
    assert_eq!(reopened.list().await.unwrap().len(), 1);
    assert_eq!(f.profiles.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn retry_after_helper_profile_failure_uses_the_saved_bot() {
    let f = Fixture::new().await;
    sqlx::query("CREATE TRIGGER reject_helper BEFORE INSERT ON agent_profiles BEGIN SELECT RAISE(FAIL, 'test failure'); END").execute(&f.pool).await.unwrap();
    let error = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap_err();
    let CreateError::Initialization {
        bot_id,
        source: Error::Database(_),
    } = error
    else {
        panic!("expected initialization database failure")
    };
    sqlx::query("DROP TRIGGER reject_helper")
        .execute(&f.pool)
        .await
        .unwrap();
    f.bots.initialize(&bot_id).await.unwrap();
    assert_eq!(
        f.profiles
            .resolve_launch(&BotService::helper_profile_id(&bot_id))
            .await
            .unwrap()
            .model
            .as_deref(),
        Some("model-a")
    );
}

#[tokio::test]
async fn invalid_bot_configuration_is_rejected_before_persistence() {
    let f = Fixture::new().await;
    for input in [
        configuration(" ", "a", "model"),
        configuration("A", "missing", "model"),
        configuration("A", "a", " "),
    ] {
        assert!(matches!(
            f.bots.create(input).await,
            Err(CreateError::Rejected(_))
        ));
    }
    assert!(f.bots.list().await.unwrap().is_empty());
    assert!(!f.root.path().join("bot").exists());
}

#[tokio::test]
async fn visible_messages_are_isolated_and_ordered_across_session_changes() {
    let f = Fixture::new().await;
    let a = f
        .bots
        .create(configuration("A", "a", "model"))
        .await
        .unwrap();
    let b = f
        .bots
        .create(configuration("B", "b", "model"))
        .await
        .unwrap();
    let first = f
        .messages
        .append(&a.bot_id, MessageKind::UserInput, "first input")
        .await
        .unwrap();
    let other = f
        .messages
        .append(&b.bot_id, MessageKind::BotReply, "other Bot reply")
        .await
        .unwrap();
    f.bots
        .set_current_session(&a.bot_id, Some("next-generation"))
        .await
        .unwrap();
    let reply = f
        .messages
        .append(&a.bot_id, MessageKind::BotReply, "reply body only")
        .await
        .unwrap();
    let messages = f.messages.list(&a.bot_id).await.unwrap();
    assert_eq!(messages, vec![first, reply]);
    assert_eq!(f.messages.list(&b.bot_id).await.unwrap(), vec![other]);
    assert!(matches!(
        f.messages
            .append("missing", MessageKind::UserInput, "input")
            .await,
        Err(Error::NotFound(_))
    ));
    for kind in ["assistant", "tool", "helper"] {
        assert!(serde_json::from_value::<MessageKind>(json!(kind)).is_err());
        assert!(
            sqlx::query(
                "INSERT INTO bot_messages (id, bot_id, kind, content) VALUES (?, ?, ?, 'internal')"
            )
            .bind(kind)
            .bind(&a.bot_id)
            .bind(kind)
            .execute(&f.pool)
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn messages_with_equal_timestamps_have_a_stable_id_order() {
    let f = Fixture::new().await;
    let bot = f
        .bots
        .create(configuration("A", "a", "model"))
        .await
        .unwrap();
    for id in ["c", "a", "b"] {
        sqlx::query("INSERT INTO bot_messages (id, bot_id, kind, content, created_at) VALUES (?, ?, 'user_input', ?, '2026-11-18T00:00:00.000Z')").bind(id).bind(&bot.bot_id).bind(id).execute(&f.pool).await.unwrap();
    }
    let messages = f.messages.list(&bot.bot_id).await.unwrap();
    assert_eq!(
        messages.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[tokio::test]
async fn deleted_helper_profile_is_reinitialized_with_bot_defaults() {
    let f = Fixture::new().await;
    let bot = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap();
    let id = BotService::helper_profile_id(&bot.bot_id);
    f.profiles.delete(&id).await.unwrap();
    assert!(f.profiles.resolve_launch(&id).await.is_err());
    f.bots.initialize(&bot.bot_id).await.unwrap();
    assert_eq!(
        f.profiles
            .resolve_launch(&id)
            .await
            .unwrap()
            .model
            .as_deref(),
        Some("model-a")
    );
}
