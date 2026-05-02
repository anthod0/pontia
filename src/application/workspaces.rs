use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkspaceRecord {
    pub(crate) workspace_id: String,
    pub(crate) canonical_path: String,
}

pub(crate) async fn upsert_workspace(
    pool: &SqlitePool,
    workspace: &str,
) -> Result<WorkspaceRecord> {
    let input_path = PathBuf::from(workspace);
    std::fs::create_dir_all(&input_path)?;
    let canonical_path = std::fs::canonicalize(&input_path)?.display().to_string();
    let display_path = canonical_path.clone();
    let name = Path::new(&canonical_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToString::to_string);

    if let Some(row) =
        sqlx::query("SELECT workspace_id, canonical_path FROM workspaces WHERE canonical_path = ?")
            .bind(&canonical_path)
            .fetch_optional(pool)
            .await?
    {
        let workspace_id: String = row.try_get("workspace_id")?;
        sqlx::query(
            r#"UPDATE workspaces
               SET display_path = ?, name = COALESCE(name, ?), state = 'active',
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workspace_id = ?"#,
        )
        .bind(&display_path)
        .bind(&name)
        .bind(&workspace_id)
        .execute(pool)
        .await?;
        return Ok(WorkspaceRecord {
            workspace_id,
            canonical_path,
        });
    }

    let workspace_id = new_workspace_id().to_string();
    sqlx::query(
        r#"INSERT INTO workspaces
           (workspace_id, canonical_path, display_path, name, last_used_at)
           VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#,
    )
    .bind(&workspace_id)
    .bind(&canonical_path)
    .bind(&display_path)
    .bind(&name)
    .execute(pool)
    .await?;

    Ok(WorkspaceRecord {
        workspace_id,
        canonical_path,
    })
}
