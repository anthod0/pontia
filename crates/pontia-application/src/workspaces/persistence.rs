use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use pontia_core::{
    error::{Error, Result},
    ids::new_workspace_id,
};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

use super::WorkspaceRecord;

pub async fn upsert_workspace(pool: &SqlitePool, workspace: &str) -> Result<WorkspaceRecord> {
    let input_path = PathBuf::from(workspace);
    std::fs::create_dir_all(&input_path)?;
    let canonical_path = std::fs::canonicalize(&input_path)?.display().to_string();
    upsert_canonical_workspace(pool, &canonical_path, None).await
}

pub(crate) async fn upsert_canonical_workspace(
    pool: &SqlitePool,
    canonical_path: &str,
    requested_name: Option<&str>,
) -> Result<WorkspaceRecord> {
    let display_path = canonical_path.to_string();
    let name = requested_name
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            Path::new(&canonical_path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        });

    let repository = SqliteWorkspaceRepository::new(pool.clone());
    if let Some(row) = repository
        .get_workspace_record_by_canonical_path(canonical_path)
        .await?
    {
        repository
            .reactivate_workspace(&row.workspace_id, &display_path, name.as_deref())
            .await?;
        return Ok(WorkspaceRecord {
            workspace_id: row.workspace_id,
            canonical_path: canonical_path.to_string(),
            name,
        });
    }

    let workspace_id = new_workspace_id().to_string();
    let insert_result = repository
        .insert_workspace(
            &workspace_id,
            canonical_path,
            &display_path,
            name.as_deref(),
        )
        .await;
    if let Err(error) = insert_result {
        if !is_unique_violation(&error) {
            return Err(error);
        }
        let row = repository
            .get_workspace_record_by_canonical_path(canonical_path)
            .await?
            .ok_or(error)?;
        repository
            .reactivate_workspace(&row.workspace_id, &display_path, name.as_deref())
            .await?;
        return Ok(WorkspaceRecord {
            workspace_id: row.workspace_id,
            canonical_path: canonical_path.to_string(),
            name,
        });
    }

    Ok(WorkspaceRecord {
        workspace_id,
        canonical_path: canonical_path.to_string(),
        name,
    })
}

fn is_unique_violation(error: &Error) -> bool {
    matches!(
        error,
        Error::Database(sqlx::Error::Database(database_error))
            if database_error.is_unique_violation()
    )
}

pub async fn get_workspace_record(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<WorkspaceRecord>> {
    SqliteWorkspaceRepository::new(pool.clone())
        .get_workspace_record(workspace_id)
        .await?
        .map(|row| {
            Ok(WorkspaceRecord {
                workspace_id: row.workspace_id,
                canonical_path: row.canonical_path,
                name: row.name,
            })
        })
        .transpose()
}
