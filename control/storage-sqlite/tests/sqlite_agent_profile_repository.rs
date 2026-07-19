use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::agent_profiles::{ExecutionProfileWriteRecord, SqliteAgentProfileRepository},
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_agent_profile_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

fn upsert_record(profile_id: &str, version: &str, name: &str) -> ExecutionProfileWriteRecord {
    ExecutionProfileWriteRecord {
        profile_id: profile_id.to_string(),
        version: version.to_string(),
        name: name.to_string(),
        description: Some(format!("{name} description")),
        supported_client_types: json!(["pi"]).to_string(),
        agent_kind: "executor".to_string(),
        system_prompt_template: Some("system".to_string()),
        turn_prompt_template: Some("turn".to_string()),
        default_session_role: Some("role".to_string()),
        default_session_description: Some("session".to_string()),
        handle_prefix: Some("agent".to_string()),
        expected_output_schema: Some("free_text".to_string()),
        artifact_contract: json!({"produces": ["summary"]}).to_string(),
        default_execution_policy: json!({"allow_file_writes": true}).to_string(),
        default_review_policy: json!({}).to_string(),
        metadata: json!({"custom": true}).to_string(),
    }
}

#[tokio::test]
async fn inserts_updates_and_gets_execution_profile_versions() {
    let pool = test_pool().await;
    let repository = SqliteAgentProfileRepository::new(pool);

    repository
        .insert_version(upsert_record("repo-profile", "1", "Repo Profile"))
        .await
        .expect("insert profile");

    let inserted = repository
        .get_version("repo-profile", "1")
        .await
        .expect("get inserted")
        .expect("profile exists");
    assert_eq!(inserted.profile_id, "repo-profile");
    assert_eq!(inserted.version, "1");
    assert_eq!(inserted.name, "Repo Profile");
    assert_eq!(inserted.supported_client_types, json!(["pi"]).to_string());
    assert!(inserted.active);

    repository
        .update_version(ExecutionProfileWriteRecord {
            profile_id: "repo-profile".to_string(),
            version: "1".to_string(),
            name: "Repo Profile Edited".to_string(),
            description: None,
            supported_client_types: json!(["pi", "generic"]).to_string(),
            agent_kind: "executor".to_string(),
            system_prompt_template: None,
            turn_prompt_template: Some("edited turn".to_string()),
            default_session_role: None,
            default_session_description: None,
            handle_prefix: Some("review".to_string()),
            expected_output_schema: Some("free_text".to_string()),
            artifact_contract: json!({}).to_string(),
            default_execution_policy: json!({"allow_file_writes": false}).to_string(),
            default_review_policy: json!({"requires_review": true}).to_string(),
            metadata: json!({"edited": true}).to_string(),
        })
        .await
        .expect("update profile");

    let updated = repository
        .get_version("repo-profile", "1")
        .await
        .expect("get updated")
        .expect("profile exists");
    assert_eq!(updated.name, "Repo Profile Edited");
    assert_eq!(updated.description, None);
    assert_eq!(updated.agent_kind, "executor");
    assert_eq!(updated.metadata, json!({"edited": true}).to_string());
}

#[tokio::test]
async fn lists_latest_and_versions_with_archive_filters() {
    let pool = test_pool().await;
    let repository = SqliteAgentProfileRepository::new(pool);

    repository
        .insert_version(upsert_record("repo-list", "1", "Repo List One"))
        .await
        .expect("insert v1");
    repository
        .insert_version(upsert_record("repo-list", "2", "Repo List Two"))
        .await
        .expect("insert v2");
    repository
        .archive_version("repo-list", "2")
        .await
        .expect("archive v2");

    let latest_active = repository
        .get_latest("repo-list")
        .await
        .expect("get latest")
        .expect("active latest exists");
    assert_eq!(latest_active.version, "1");

    let active_versions = repository
        .list_versions("repo-list", false)
        .await
        .expect("list active versions");
    assert_eq!(active_versions.len(), 1);
    assert_eq!(active_versions[0].version, "1");

    let all_versions = repository
        .list_versions("repo-list", true)
        .await
        .expect("list all versions");
    assert_eq!(all_versions.len(), 2);
    assert_eq!(all_versions[1].version, "2");
    assert!(!all_versions[1].active);
    assert_eq!(
        all_versions[1].archived_reason.as_deref(),
        Some("deleted via External API")
    );

    let latest_including_archived = repository
        .list_latest_including_archived()
        .await
        .expect("list latest including archived");
    let archived_latest = latest_including_archived
        .iter()
        .find(|profile| profile.profile_id == "repo-list")
        .expect("repo-list latest exists");
    assert_eq!(archived_latest.version, "2");
}

#[tokio::test]
async fn archives_active_versions_and_reports_affected_rows() {
    let pool = test_pool().await;
    let repository = SqliteAgentProfileRepository::new(pool);

    repository
        .insert_version(upsert_record("repo-archive", "1", "Repo Archive One"))
        .await
        .expect("insert v1");
    repository
        .insert_version(upsert_record("repo-archive", "2", "Repo Archive Two"))
        .await
        .expect("insert v2");

    assert!(
        repository
            .profile_exists("repo-archive")
            .await
            .expect("profile exists")
    );
    let archived = repository
        .archive_active_versions("repo-archive")
        .await
        .expect("archive active versions");
    assert_eq!(archived, 2);

    let active_latest = repository
        .list_latest()
        .await
        .expect("list latest")
        .into_iter()
        .find(|profile| profile.profile_id == "repo-archive");
    assert!(active_latest.is_none());
}
