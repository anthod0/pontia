pub(super) use pontia_agent_control::workspaces::path_policy::{
    canonical_root, resolve_relative_path,
};

pub(super) fn should_skip_directory(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target")
}
