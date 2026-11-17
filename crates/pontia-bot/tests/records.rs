use std::sync::Arc;

use pontia_agent_control::{
    AgentProfileService, UpsertExecutionProfileRequest,
    client_contract::{ClientConfiguration, ClientProfile, test_registration},
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
impl ClientProfile for Profile {
    fn validate_templates(&self, _: Option<&str>, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn bind(&self, profile: &pontia_agent_control::ExecutionProfileView) -> Result<Value> {
        Ok(json!({"system": profile.system_prompt_template}))
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
        let mut registry = ClientRegistry::default();
        let mut client = test_registration();
        client.configuration = Some(Arc::new(Configuration));
        client.profile = Some(Arc::new(Profile));
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

    async fn profile_request(&self, id: &str) -> UpsertExecutionProfileRequest {
        let profile = self.profiles.get_latest(id).await.unwrap().unwrap();
        serde_json::from_value(serde_json::to_value(profile).unwrap()).unwrap()
    }
}

fn configuration(name: &str, key: &str, model: &str) -> BotConfiguration {
    BotConfiguration {
        name: name.into(),
        description: Some(format!("{name} description")),
        client_type: "generic".into(),
        client_key: key.into(),
        model: model.into(),
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
    assert!(directory.is_dir());
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
    let initial = f.profiles.launch_defaults(&helper, None).await.unwrap();
    assert_eq!(
        (
            initial.client_type.as_str(),
            initial.client_key.as_str(),
            initial.model.as_str()
        ),
        ("generic", "a", "model-a")
    );
    assert_eq!(
        f.profiles
            .launch_defaults(DREAMING_PROFILE_ID, None)
            .await
            .unwrap()
            .model,
        "model-a"
    );
    for id in [&helper, DREAMING_PROFILE_ID] {
        let mut request = f.profile_request(id).await;
        request.default_client_key = Some("b".into());
        request.default_model = Some("custom-model".into());
        request.system_prompt_template = Some("custom instructions".into());
        f.profiles.update_version(id, "1", request).await.unwrap();
    }
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
            .launch_defaults(&BotService::helper_profile_id(&b.bot_id), None)
            .await
            .unwrap()
            .model,
        "model-b"
    );
    for id in [&helper, DREAMING_PROFILE_ID] {
        assert_eq!(
            f.profiles.launch_defaults(id, None).await.unwrap().model,
            "custom-model"
        );
        let binding = f
            .profiles
            .resolve_for_client("generic", Some(id), Some("1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(binding.binding["system"], "custom instructions");
    }
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
    assert_eq!(
        f.profiles
            .list_versions(DREAMING_PROFILE_ID, true)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn retry_after_profile_failure_preserves_the_already_created_helper() {
    let f = Fixture::new().await;
    sqlx::query("CREATE TRIGGER reject_dreaming BEFORE INSERT ON execution_profiles WHEN NEW.profile_id = 'bot-dreaming' BEGIN SELECT RAISE(FAIL, 'test failure'); END").execute(&f.pool).await.unwrap();
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
    let helper = BotService::helper_profile_id(&bot_id);
    let mut request = f.profile_request(&helper).await;
    request.default_model = Some("user-edited-model".into());
    f.profiles
        .update_version(&helper, "1", request)
        .await
        .unwrap();
    sqlx::query("DROP TRIGGER reject_dreaming")
        .execute(&f.pool)
        .await
        .unwrap();
    f.bots.initialize(&bot_id).await.unwrap();
    assert_eq!(
        f.profiles
            .launch_defaults(&helper, None)
            .await
            .unwrap()
            .model,
        "user-edited-model"
    );
    assert_eq!(
        f.profiles
            .launch_defaults(DREAMING_PROFILE_ID, None)
            .await
            .unwrap()
            .model,
        "model-a"
    );
    assert_eq!(
        f.profiles.list_versions(&helper, true).await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn concurrent_creations_share_one_dreaming_profile() {
    let f = Fixture::new().await;
    let other = BotService::new(f.pool.clone(), f.root.path(), f.registry.clone());
    let (a, b) = tokio::join!(
        f.bots.create(configuration("A", "a", "model-a")),
        other.create(configuration("B", "b", "model-b"))
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        f.profiles
            .list_versions(DREAMING_PROFILE_ID, true)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(f.bots.list().await.unwrap().len(), 2);
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
async fn profile_launch_defaults_are_validated_versioned_and_editable() {
    let f = Fixture::new().await;
    let bot = f
        .bots
        .create(configuration("A", "a", "model-a"))
        .await
        .unwrap();
    let id = BotService::helper_profile_id(&bot.bot_id);
    let mut request = f.profile_request(&id).await;
    request.default_model = None;
    assert!(matches!(
        f.profiles.update_version(&id, "1", request).await,
        Err(Error::Domain(_))
    ));
    assert!(
        sqlx::query("UPDATE execution_profiles SET default_model = NULL WHERE profile_id = ?")
            .bind(&id)
            .execute(&f.pool)
            .await
            .is_err()
    );
    let mut request = f.profile_request(&id).await;
    request.version = "2".into();
    request.default_client_key = Some("b".into());
    request.default_model = Some("model-b".into());
    f.profiles
        .create_profile_version(&id, request)
        .await
        .unwrap();
    let latest = f.profiles.launch_defaults(&id, None).await.unwrap();
    assert_eq!(
        (
            latest.version.as_str(),
            latest.client_key.as_str(),
            latest.model.as_str()
        ),
        ("2", "b", "model-b")
    );
    assert_eq!(
        f.profiles
            .launch_defaults(&id, Some("1"))
            .await
            .unwrap()
            .model,
        "model-a"
    );
    f.profiles.archive_version(&id, "2").await.unwrap();
    assert!(f.profiles.launch_defaults(&id, Some("2")).await.is_err());
    f.bots.initialize(&bot.bot_id).await.unwrap();
    assert_eq!(f.profiles.list_versions(&id, true).await.unwrap().len(), 2);
}
