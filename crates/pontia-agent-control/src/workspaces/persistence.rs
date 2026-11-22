use std::path::Path;

use sqlx::SqlitePool;

use pontia_core::{
    error::{Error, Result},
    ids::new_workspace_id,
};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

use super::WorkspaceRecord;

pub(crate) async fn resolve_session_directory(
    pool: &SqlitePool,
    pontia_home: &Path,
    directory: &str,
) -> Result<(String, Option<WorkspaceRecord>)> {
    let canonical = std::fs::canonicalize(directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound(format!("Workspace directory {directory} not found"))
        } else {
            Error::Io(error)
        }
    })?;
    if !canonical.is_dir() {
        return Err(Error::Domain(
            "Session launch path must be a directory".into(),
        ));
    }
    let bot_root = pontia_home.join("bot");
    if bot_root.is_dir() && canonical.starts_with(std::fs::canonicalize(bot_root)?) {
        return Ok((canonical.to_string_lossy().into_owned(), None));
    }
    let workspace = require_active_workspace(pool, &canonical.to_string_lossy()).await?;
    Ok((workspace.canonical_path.clone(), Some(workspace)))
}

pub async fn require_active_workspace(
    pool: &SqlitePool,
    workspace: &str,
) -> Result<WorkspaceRecord> {
    let canonical = std::fs::canonicalize(workspace).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound(format!("Workspace directory {workspace} not found"))
        } else {
            Error::Io(error)
        }
    })?;
    if !canonical.is_dir() {
        return Err(Error::Domain("Workspace must be a directory".into()));
    }
    let path = canonical.display().to_string();
    let row = SqliteWorkspaceRepository::new(pool.clone())
        .get_workspace_record_by_canonical_path(&path)
        .await?
        .ok_or_else(|| {
            Error::NotFound(format!(
                "Workspace {path} must be registered and activated first"
            ))
        })?;
    require_active_workspace_id(pool, &row.workspace_id).await
}

pub async fn require_active_workspace_id(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<WorkspaceRecord> {
    let row = SqliteWorkspaceRepository::new(pool.clone())
        .get_workspace(workspace_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?;
    if row.state != "active" {
        return Err(Error::StateConflict(format!(
            "Workspace {workspace_id} must be activated first"
        )));
    }
    Ok(WorkspaceRecord {
        workspace_id: row.workspace_id,
        canonical_path: row.canonical_path,
        name: row.name,
        description: row.description,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceRegistration {
    Inserted,
    Reactivated,
    AlreadyActive,
}

pub async fn upsert_canonical_workspace(
    pool: &SqlitePool,
    canonical_path: &str,
    requested_name: Option<&str>,
) -> Result<(WorkspaceRecord, WorkspaceRegistration)> {
    let name = requested_name
        .filter(|name| !name.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            Path::new(&canonical_path)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        });

    let (row, previous_state) = SqliteWorkspaceRepository::new(pool.clone())
        .register_workspace(
            &new_workspace_id().to_string(),
            canonical_path,
            name.as_deref(),
        )
        .await?;
    let registration = match previous_state.as_deref() {
        None => WorkspaceRegistration::Inserted,
        Some("deleted") => WorkspaceRegistration::Reactivated,
        Some(_) => WorkspaceRegistration::AlreadyActive,
    };
    Ok((
        WorkspaceRecord {
            workspace_id: row.workspace_id,
            canonical_path: row.canonical_path,
            name: row.name,
            description: row.description,
        },
        registration,
    ))
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
                description: row.description,
            })
        })
        .transpose()
}
