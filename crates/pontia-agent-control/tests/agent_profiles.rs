use pontia_agent_control::{
    AgentProfileService, UpsertAgentProfileRequest,
    client_contract::{
        ClientConfiguration, ClientInstructions, InstructionsBindingInput, test_registration,
    },
    clients::ClientRegistry,
};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::{
    connect_sqlite, repositories::agent_clients::SqliteAgentClientRepository, run_migrations,
};
use serde_json::{Value, json};
use std::sync::Arc;

struct Configuration;
impl ClientConfiguration for Configuration {
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
    fn resolve(&self, key: &str) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain("Unknown client key".into()));
        }
        Ok(json!({}))
    }
}
struct Instructions;
impl ClientInstructions for Instructions {
    fn inject_standalone_session_instructions(&self) -> bool {
        false
    }
    fn validate_instructions(&self, text: Option<&str>) -> Result<()> {
        if text == Some("client-rejected") {
            return Err(Error::Domain("Unsupported instructions".into()));
        }
        Ok(())
    }
    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<Value> {
        Ok(json!({"system_prompt":input.system_prompt}))
    }
}
async fn fixture_with_changes(
    changes: pontia_agent_control::ControlChanges,
) -> (
    AgentProfileService,
    SqliteAgentClientRepository,
    sqlx::SqlitePool,
) {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let settings = SqliteAgentClientRepository::new(pool.clone());
    settings.save("generic", "default", true).await.unwrap();
    let mut client = test_registration();
    client.configuration = Some(Arc::new(Configuration));
    client.instructions = Some(Arc::new(Instructions));
    let mut clients = ClientRegistry::default();
    clients.register(client);
    (
        AgentProfileService::new(pool.clone(), clients, changes, std::path::Path::new(".")),
        settings,
        pool,
    )
}
async fn fixture() -> (
    AgentProfileService,
    SqliteAgentClientRepository,
    sqlx::SqlitePool,
) {
    fixture_with_changes(Default::default()).await
}

#[tokio::test]
async fn updating_only_the_model_does_not_publish_a_profile_notice() {
    let changes = pontia_agent_control::ControlChanges::default();
    let mut events = changes.subscribe();
    let (service, _, _) = fixture_with_changes(changes).await;
    service.create(configured(), None).await.unwrap();
    assert!(matches!(
        events.try_recv(),
        Ok(pontia_agent_control::ControlChange::AgentProfileCreated { .. })
    ));
    let mut updated = configured();
    updated.model = Some("new-model".into());
    service.update("reviewer", updated, None).await.unwrap();
    assert_eq!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    );
}

fn request() -> UpsertAgentProfileRequest {
    UpsertAgentProfileRequest {
        profile_id: "reviewer".into(),
        name: "Reviewer".into(),
        description: None,
        client_type: None,
        client_key: None,
        model: None,
        reasoning_effort: None,
        system_prompt: Some("Review carefully".into()),
    }
}
fn configured() -> UpsertAgentProfileRequest {
    UpsertAgentProfileRequest {
        client_type: Some("generic".into()),
        client_key: Some("default".into()),
        model: Some("model".into()),
        ..request()
    }
}

#[tokio::test]
async fn crud_and_launch_use_current_profile() {
    let (service, _, _) = fixture().await;
    let initial = service.create(request(), None).await.unwrap();
    assert!(matches!(
        service.create(request(), None).await,
        Err(Error::StateConflict(_))
    ));
    assert_eq!(service.get("reviewer").await.unwrap().unwrap(), initial);
    let mut edited = configured();
    edited.system_prompt = Some("Edited".into());
    let updated = service.update("reviewer", edited, None).await.unwrap();
    assert_eq!(updated.created_at, initial.created_at);
    assert_eq!(service.list().await.unwrap(), vec![updated]);
    let launch = service.resolve_launch("reviewer").await.unwrap();
    assert_eq!(launch.client_type.as_deref(), Some("generic"));
    assert_eq!(launch.client_key.as_deref(), Some("default"));
    assert_eq!(launch.model.as_deref(), Some("model"));
    assert_eq!(launch.system_prompt.as_deref(), Some("Edited"));
    service.delete("reviewer", None).await.unwrap();
    assert!(matches!(
        service.resolve_launch("reviewer").await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        service.update("reviewer", request(), None).await,
        Err(Error::NotFound(_))
    ));
    assert!(matches!(
        service.delete("reviewer", None).await,
        Err(Error::NotFound(_))
    ));
    assert!(service.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_empty_identifiers_partial_defaults_and_invalid_instructions() {
    let (service, _, _) = fixture().await;
    let mut invalid = Vec::new();
    invalid.push(UpsertAgentProfileRequest {
        reasoning_effort: Some("high".into()),
        ..request()
    });
    let mut input = request();
    input.profile_id = " ".into();
    invalid.push(input);
    let mut input = request();
    input.name = " ".into();
    invalid.push(input);
    for mask in 1..7 {
        let mut input = request();
        if mask & 1 != 0 {
            input.client_type = Some("generic".into());
        }
        if mask & 2 != 0 {
            input.client_key = Some("default".into());
        }
        if mask & 4 != 0 {
            input.model = Some("model".into());
        }
        invalid.push(input);
    }
    for text in [" ", "{{input}}", "closing}}"] {
        let mut input = request();
        input.system_prompt = Some(text.into());
        invalid.push(input);
    }
    for field in ["client", "key", "model"] {
        let mut input = configured();
        match field {
            "client" => input.client_type = Some(" ".into()),
            "key" => input.client_key = Some(" ".into()),
            _ => input.model = Some(" ".into()),
        }
        invalid.push(input);
    }
    let mut input = configured();
    input.system_prompt = Some("client-rejected".into());
    invalid.push(input);
    for input in invalid {
        assert!(matches!(
            service.create(input, None).await,
            Err(Error::Domain(_))
        ));
    }
    assert!(service.list().await.unwrap().is_empty());
    assert!(matches!(
        service.update("other", request(), None).await,
        Err(Error::Domain(_))
    ));
}

#[tokio::test]
async fn creates_profiles_only_with_handwritable_ids() {
    let (service, _, _) = fixture().await;
    for id in [
        "codex-default",
        "bot_01a1f755-70e8-7633-9528-31207e0f73c5-helper",
        "0-review.v2",
    ] {
        let mut input = request();
        input.profile_id = id.into();
        service.create(input, None).await.unwrap();
    }
    for id in [
        "Reviewer",
        "-review",
        "_review",
        ".review",
        "code review",
        "a/b",
        "评审",
    ] {
        let mut input = request();
        input.profile_id = id.into();
        assert!(
            matches!(service.create(input, None).await, Err(Error::Domain(_))),
            "{id}"
        );
    }
    assert_eq!(service.list().await.unwrap().len(), 3);
}

#[tokio::test]
async fn existing_profiles_with_older_ids_can_be_edited_launched_and_deleted() {
    let (service, _, pool) = fixture().await;
    sqlx::query(
        "INSERT INTO agent_profiles (profile_id, name, client_type, client_key, model)
         VALUES ('Old Reviewer', 'Old', 'generic', 'default', 'model')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut edited = configured();
    edited.profile_id = "Old Reviewer".into();
    edited.name = "Edited".into();
    assert_eq!(
        service
            .update("Old Reviewer", edited, None)
            .await
            .unwrap()
            .name,
        "Edited"
    );
    assert_eq!(
        service
            .resolve_launch("Old Reviewer")
            .await
            .unwrap()
            .model
            .as_deref(),
        Some("model")
    );
    service.delete("Old Reviewer", None).await.unwrap();
    assert!(service.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_unknown_and_disabled_clients_at_save_and_launch() {
    let (service, settings, _) = fixture().await;
    let mut input = configured();
    input.client_type = Some("missing".into());
    assert!(service.create(input, None).await.is_err());
    settings.save("generic", "unknown", true).await.unwrap();
    let mut input = configured();
    input.client_key = Some("unknown".into());
    assert!(service.create(input, None).await.is_err());
    service.create(configured(), None).await.unwrap();
    settings.save("generic", "default", false).await.unwrap();
    assert!(matches!(
        service.resolve_launch("reviewer").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert!(matches!(
        service.update("reviewer", configured(), None).await,
        Err(Error::CapabilityUnavailable(_))
    ));
    service.delete("reviewer", None).await.unwrap();
    assert!(matches!(
        service.create(configured(), None).await,
        Err(Error::CapabilityUnavailable(_))
    ));
}

#[tokio::test]
async fn rejects_launch_defaults_for_clients_without_configuration() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    SqliteAgentClientRepository::new(pool.clone())
        .save("generic", "default", true)
        .await
        .unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(test_registration());
    let service =
        AgentProfileService::new(pool, clients, Default::default(), std::path::Path::new("."));
    let mut input = configured();
    input.system_prompt = None;
    assert!(matches!(
        service.create(input, None).await,
        Err(Error::CapabilityUnavailable(_))
    ));
}

fn configured_profile(id: &str, model: &str) -> UpsertAgentProfileRequest {
    UpsertAgentProfileRequest {
        profile_id: id.into(),
        name: id.into(),
        model: Some(model.into()),
        ..configured()
    }
}

#[tokio::test]
async fn launchable_profiles_follow_current_launch_admission_in_id_order() {
    let (service, settings, pool) = fixture().await;
    service
        .create(configured_profile("zeta", "model-z"), None)
        .await
        .unwrap();
    let mut described = configured_profile("alpha", "model-a");
    described.description = Some("Reviews code".into());
    service.create(described, None).await.unwrap();
    service.create(request(), None).await.unwrap();
    service
        .create(configured_profile("unparsable", "model-u"), None)
        .await
        .unwrap();
    settings.save("generic", "unknown", true).await.unwrap();
    // The client configuration no longer accepts this saved key.
    sqlx::query("UPDATE agent_profiles SET client_key = 'unknown' WHERE profile_id = 'unparsable'")
        .execute(&pool)
        .await
        .unwrap();
    let listed = service.list_launchable().await.unwrap();
    assert_eq!(
        listed
            .iter()
            .map(|profile| (profile.profile_id.as_str(), profile.model.as_str()))
            .collect::<Vec<_>>(),
        [("alpha", "model-a"), ("zeta", "model-z")]
    );
    assert_eq!(listed[0].description.as_deref(), Some("Reviews code"));
    assert_eq!(
        (
            listed[0].client_type.as_str(),
            listed[0].client_key.as_str()
        ),
        ("generic", "default")
    );
    let mut updated = configured_profile("zeta", "model-new");
    updated.description = Some("Updated".into());
    service.update("zeta", updated, None).await.unwrap();
    service.delete("alpha", None).await.unwrap();
    let listed = service.list_launchable().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].model.as_str(), listed[0].description.as_deref()),
        ("model-new", Some("Updated"))
    );
    settings.save("generic", "default", false).await.unwrap();
    assert!(service.list_launchable().await.unwrap().is_empty());
}

#[tokio::test]
async fn launchable_profiles_report_storage_failures() {
    let (service, _, pool) = fixture().await;
    service.create(configured(), None).await.unwrap();
    sqlx::query("DROP TABLE agent_clients")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        service.list_launchable().await,
        Err(Error::Database(_))
    ));
}
