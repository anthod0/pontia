use std::{
    env,
    path::{Path, PathBuf},
};

mod archive;
mod remote_cache;
mod source;
mod static_serving;

pub use source::resolve_dashboard;
pub use static_serving::{dashboard, dashboard_asset, dashboard_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDashboard {
    root: Option<PathBuf>,
    unavailable_reason: Option<String>,
}

impl ResolvedDashboard {
    pub fn local_default() -> Self {
        Self::available(default_dist_path())
    }

    pub fn available(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            unavailable_reason: None,
        }
    }

    pub fn unavailable(reason: String) -> Self {
        Self {
            root: None,
            unavailable_reason: Some(reason),
        }
    }

    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    fn unavailable_message(&self) -> String {
        self.unavailable_reason.clone().unwrap_or_else(|| {
            "Dashboard frontend has not been built. Run `pnpm --dir=apps/dashboard run build`."
                .to_string()
        })
    }
}

fn default_dist_path() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("apps/dashboard/dist")
}
