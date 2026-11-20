use pontia_agent_control::{
    AgentProfileService, clients::ClientRegistry, standalone_session_instructions,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn profiles_compose_once_and_pin_the_selected_version() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    let service = AgentProfileService::new(pool.clone()).with_clients(clients);
    let standalone = standalone_session_instructions();
    for (version, prompt) in [("1", "first-marker"), ("2", "second-marker")] {
        sqlx::query("INSERT INTO execution_profiles (profile_id,version,name,supported_client_types,system_prompt_template,default_execution_policy) VALUES ('reviewer',?,'Reviewer','[\"pi\"]',?,'{\"allow_file_writes\":false}')")
            .bind(version).bind(prompt).execute(&pool).await.unwrap();
    }
    for (id, version, expected, selected) in [
        (None, None, standalone.to_owned(), None),
        (
            Some("reviewer"),
            None,
            format!("{standalone}\n\nsecond-marker"),
            Some("2"),
        ),
        (
            Some("reviewer"),
            Some("1"),
            format!("{standalone}\n\nfirst-marker"),
            Some("1"),
        ),
    ] {
        let resolved = service
            .resolve_for_client("pi", id, version, None, true)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.binding, json!({"system_prompt":expected}));
        assert_eq!(resolved.version.as_deref(), selected);
    }
    let combined = service
        .resolve_for_client(
            "pi",
            Some("reviewer"),
            Some("1"),
            Some("caller-marker"),
            true,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        combined.binding["system_prompt"],
        format!("{standalone}\n\nfirst-marker\n\ncaller-marker")
    );
    assert_eq!(combined.version.as_deref(), Some("1"));

    let without_standalone = service
        .resolve_for_client(
            "pi",
            Some("reviewer"),
            Some("1"),
            Some("caller-marker"),
            false,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        without_standalone.binding["system_prompt"],
        "first-marker\n\ncaller-marker"
    );
    assert_eq!(without_standalone.version.as_deref(), Some("1"));

    let caller_only = service
        .resolve_for_client("pi", None, None, Some("caller-marker"), false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(caller_only.binding["system_prompt"], "caller-marker");
    assert_eq!(caller_only.version, None);

    assert!(
        service
            .resolve_for_client("pi", None, None, None, false)
            .await
            .unwrap()
            .is_none()
    );
}
