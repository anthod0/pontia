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
    fn merged(&self, _: &str) -> Value {
        json!({})
    }
    fn resolve(&self, key: &str) -> Result<Value> {
        if !self.keys().iter().any(|known| known == key) {
            return Err(Error::Domain("Unknown client key".into()));
        }
        Ok(json!({}))
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
            settings.save("generic", key, true).await.unwrap();
        }

        let mut registry = ClientRegistry::default();
        let mut client = test_registration();
        client.configuration = Some(Arc::new(Configuration));
        client.instructions = Some(Arc::new(Profile));
        registry.register(client);
        Self {
            bots: BotService::new(pool.clone(), root.path(), registry.clone()),
            messages: MessageService::new(pool.clone()),
            profiles: AgentProfileService::new(pool.clone(), registry.clone(), Default::default()),
            root,
            pool,
            registry,
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
    let bot = f.bots.create(input.clone(), Locale::En).await.unwrap();
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
    let bot = f.bots.create(input, Locale::En).await.unwrap();
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
        .create(configuration("A", "a", "model-a"), Locale::En)
        .await
        .unwrap();
    let mut input = configuration("B", "b", "model-b");
    input.description = None;
    let b = f.bots.create(input, Locale::En).await.unwrap();
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
async fn bot_lifecycle_preserves_user_information_and_writes_no_agent_profiles() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.root.path().join("bot")).unwrap();
    let user = f.root.path().join("bot/USER.md");
    std::fs::write(&user, "existing user information").unwrap();
    let bot = f
        .bots
        .create(configuration("A", "a", "model-a"), Locale::En)
        .await
        .unwrap();
    f.bots
        .update(&bot.bot_id, configuration("Changed", "b", "model-new"))
        .await
        .unwrap();
    f.bots.initialize(&bot.bot_id).await.unwrap();
    assert!(f.profiles.list().await.unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(user).unwrap(),
        "existing user information"
    );
}

#[tokio::test]
async fn initialization_leaves_a_deleted_legacy_helper_profile_deleted() {
    let f = Fixture::new().await;
    let bot = f
        .bots
        .create(configuration("A", "a", "model-a"), Locale::En)
        .await
        .unwrap();
    let legacy = format!("{}-helper", bot.bot_id);
    f.profiles
        .create(
            UpsertAgentProfileRequest {
                profile_id: legacy.clone(),
                name: "A helper".into(),
                description: None,
                client_type: Some("generic".into()),
                client_key: Some("a".into()),
                model: Some("model-a".into()),
                system_prompt: None,
            },
            None,
        )
        .await
        .unwrap();
    f.profiles.delete(&legacy, None).await.unwrap();
    f.bots.initialize(&bot.bot_id).await.unwrap();
    f.bots
        .update(&bot.bot_id, configuration("A", "b", "model-b"))
        .await
        .unwrap();
    assert!(f.profiles.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn filesystem_initialization_failure_can_be_retried_with_the_saved_bot_id() {
    let f = Fixture::new().await;
    let user = f.root.path().join("bot/USER.md");
    std::fs::create_dir_all(&user).unwrap();
    let error = f
        .bots
        .create(configuration("A", "a", "model-a"), Locale::En)
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
    let greeting = f.messages.list(&bot_id).await.unwrap();
    assert_eq!(greeting.len(), 1);
    std::fs::remove_dir(&user).unwrap();
    f.bots.initialize(&bot_id).await.unwrap();
    assert_eq!(std::fs::read_to_string(&user).unwrap(), "");
    std::fs::write(&user, "user edited this").unwrap();
    let reopened = BotService::new(f.pool.clone(), f.root.path(), f.registry.clone());
    reopened.initialize(&bot_id).await.unwrap();
    assert_eq!(std::fs::read_to_string(user).unwrap(), "user edited this");
    assert_eq!(f.messages.list(&bot_id).await.unwrap(), greeting);
    assert_eq!(reopened.list().await.unwrap().len(), 1);
    assert!(f.profiles.list().await.unwrap().is_empty());
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
            f.bots.create(input, Locale::En).await,
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
        .create(configuration("A", "a", "model"), Locale::En)
        .await
        .unwrap();
    let b = f
        .bots
        .create(configuration("B", "b", "model"), Locale::En)
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
    assert_eq!(messages[1..], [first, reply]);
    assert_eq!(f.messages.list(&b.bot_id).await.unwrap()[1..], [other]);
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
        .create(configuration("A", "a", "model"), Locale::En)
        .await
        .unwrap();
    for id in ["c", "a", "b"] {
        sqlx::query("INSERT INTO bot_messages (id, bot_id, kind, content, created_at) VALUES (?, ?, 'user_input', ?, '2026-11-18T00:00:00.000Z')").bind(id).bind(&bot.bot_id).bind(id).execute(&f.pool).await.unwrap();
    }
    let messages = f.messages.list(&bot.bot_id).await.unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.kind == MessageKind::UserInput)
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
}

#[tokio::test]
async fn a_failed_greeting_rolls_back_bot_creation() {
    let f = Fixture::new().await;
    sqlx::query(
        "CREATE TRIGGER reject_greeting BEFORE INSERT ON bot_messages
                 BEGIN SELECT RAISE(ABORT, 'message write failed'); END",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let result = f
        .bots
        .create(configuration("A", "a", "model"), Locale::En)
        .await;
    assert!(matches!(result, Err(CreateError::Rejected(_))));
    assert!(f.bots.list().await.unwrap().is_empty());
}
