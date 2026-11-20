use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use std::borrow::Cow;

#[tokio::test]
async fn migrates_latest_unarchived_profiles_and_preserves_workflow_definitions() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let old = sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|m| m.version < 53)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    old.run(&pool).await.unwrap();
    for (id, version, active, archived) in [
        ("keep", "9", 1, None),
        ("keep", "2", 1, None),
        ("keep", "3", 0, None),
        ("keep", "4", 1, Some("2026-11-20")),
        ("gone", "1", 0, Some("2026-11-20")),
    ] {
        sqlx::query("INSERT INTO execution_profiles (profile_id, version, name, description, default_client_type, default_client_key, default_model, system_prompt_template, active, archived_at, created_at, updated_at) VALUES (?, ?, ?, 'description', 'pi', 'default', 'model', 'instructions', ?, ?, 'created', 'updated')")
            .bind(id).bind(version).bind(format!("Name {version}")).bind(active).bind(archived).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO workflows (workflow_id,title,cwd,state) VALUES ('wf_test','Test','/tmp','created')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO workflow_nodes (node_id,workflow_id,node_type,phase,title,instructions,inputs,output,execution_profile_id,execution_profile_version) VALUES ('node','wf_test','agent','Work','Test','Instructions','[]','out.md','keep','9')").execute(&pool).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let rows: Vec<String> = sqlx::query_scalar("SELECT json_array(profile_id,name,description,client_type,client_key,model,system_prompt,created_at,updated_at) FROM agent_profiles")
        .fetch_all(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&rows[0]).unwrap(),
        serde_json::json!([
            "keep",
            "Name 2",
            "description",
            "pi",
            "default",
            "model",
            "instructions",
            "created",
            "updated"
        ])
    );
    let removed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE name LIKE '%execution_profiles%'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(removed, 0);
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('workflow_nodes')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!columns.iter().any(|c| c == "execution_profile_version"));
    for (field, value) in [
        ("workflow_id", "'other'"),
        ("parent_node_id", "'other'"),
        ("node_type", "'control'"),
        ("phase", "'Changed'"),
        ("title", "'Changed'"),
        ("instructions", "'Changed'"),
        ("inputs", "'[\"other.md\"]'"),
        ("output", "'other.md'"),
        ("execution_profile_id", "'other'"),
        ("introduced_revision", "2"),
    ] {
        let error = sqlx::query(&format!(
            "UPDATE workflow_nodes SET {field}={value} WHERE node_id='node'"
        ))
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("accepted workflow node definitions are immutable"),
            "{field}: {error}"
        );
    }
    sqlx::query("UPDATE workflow_nodes SET session_id='created-session' WHERE node_id='node'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "ok"
    );
}
