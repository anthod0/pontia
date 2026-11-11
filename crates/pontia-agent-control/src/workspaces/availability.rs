use std::{
    fs, io,
    path::{Path, PathBuf},
};

use pontia_config::WorkspaceBrowserConfig;

use crate::views::workspaces::{
    WorkspaceAvailability, WorkspaceCheckFailure, WorkspaceUnavailableReason,
};

pub(crate) struct WorkspaceAvailabilityProbe {
    roots: Vec<(String, PathBuf)>,
    unresolved_roots: bool,
}

impl WorkspaceAvailabilityProbe {
    pub(crate) fn new(config: WorkspaceBrowserConfig) -> Self {
        let mut roots = Vec::new();
        let mut unresolved_roots = false;
        for root in config.roots {
            match fs::canonicalize(root.path).and_then(|path| {
                if fs::metadata(&path)?.is_dir() {
                    Ok(path)
                } else {
                    Err(io::ErrorKind::NotADirectory.into())
                }
            }) {
                Ok(path) => roots.push((root.root_id, path)),
                Err(_) => unresolved_roots = true,
            }
        }
        Self {
            roots,
            unresolved_roots,
        }
    }

    pub(crate) fn check(&self, path: &str) -> WorkspaceAvailability {
        let canonical = fs::canonicalize(path);
        let resolved_path = canonical.as_deref().unwrap_or_else(|_| Path::new(path));
        let root = self
            .roots
            .iter()
            .filter(|(_, root)| resolved_path.starts_with(root))
            .max_by_key(|(_, root)| root.components().count());
        let Some((root_id, root_path)) = root else {
            return if self.unresolved_roots {
                WorkspaceAvailability::Unknown {
                    reason: WorkspaceCheckFailure::RootUnavailable,
                }
            } else {
                WorkspaceAvailability::Unavailable {
                    reason: WorkspaceUnavailableReason::OutsideRoot,
                }
            };
        };
        let canonical = match canonical {
            Ok(path) => path,
            Err(error) => return io_failure(error),
        };
        match fs::metadata(&canonical) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return WorkspaceAvailability::Unavailable {
                    reason: WorkspaceUnavailableReason::NotDirectory,
                };
            }
            Err(error) => return io_failure(error),
        }
        if let Err(error) = fs::read_dir(&canonical) {
            return io_failure(error);
        }
        WorkspaceAvailability::Available {
            root_id: root_id.clone(),
            path: path_to_api_relative(canonical.strip_prefix(root_path).expect("matched root")),
        }
    }
}

fn io_failure(error: io::Error) -> WorkspaceAvailability {
    let reason = match error.kind() {
        io::ErrorKind::NotFound => WorkspaceUnavailableReason::MissingDirectory,
        io::ErrorKind::NotADirectory => WorkspaceUnavailableReason::NotDirectory,
        io::ErrorKind::PermissionDenied => WorkspaceUnavailableReason::PermissionDenied,
        _ => {
            return WorkspaceAvailability::Unknown {
                reason: WorkspaceCheckFailure::CheckFailed,
            };
        }
    };
    WorkspaceAvailability::Unavailable { reason }
}

pub fn path_to_api_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
