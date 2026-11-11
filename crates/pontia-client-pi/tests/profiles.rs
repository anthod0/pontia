use pontia_agent_control::{AgentProfileService, clients::ClientRegistry};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn profiles_compose_once_and_pin_the_selected_version() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    let service = AgentProfileService::new(pool.clone()).with_clients(clients);
    sqlx::query("UPDATE execution_profiles SET system_prompt_template='builtin-marker' WHERE profile_id='default' AND version='2'")
        .execute(&pool).await.unwrap();
    for (version, prompt) in [("1", "first-marker"), ("2", "second-marker")] {
        sqlx::query("INSERT INTO execution_profiles (profile_id,version,name,supported_client_types,system_prompt_template,default_execution_policy) VALUES ('reviewer',?,'Reviewer','[\"pi\"]',?,'{\"allow_file_writes\":false}')")
            .bind(version).bind(prompt).execute(&pool).await.unwrap();
    }
    for (id, version, expected, selected) in [
        (None, None, "builtin-marker", None),
        (
            Some("reviewer"),
            None,
            "builtin-marker\n\nsecond-marker",
            Some("2"),
        ),
        (
            Some("reviewer"),
            Some("1"),
            "builtin-marker\n\nfirst-marker",
            Some("1"),
        ),
        (Some("default"), Some("2"), "builtin-marker", Some("2")),
    ] {
        let resolved = service
            .resolve_for_client("pi", id, version)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.binding, json!({"system_prompt":expected}));
        assert_eq!(resolved.version.as_deref(), selected);
    }
    let old_builtin = service.get_version("default", "1").await.unwrap().unwrap();
    let resolved = service
        .resolve_for_client("pi", Some("default"), Some("1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        resolved.binding["system_prompt"],
        json!(old_builtin.system_prompt_template)
    );
    assert_eq!(resolved.version.as_deref(), Some("1"));
}
