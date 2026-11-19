pub mod conversation;
pub mod dreaming;
pub mod handoff;
pub mod helpers;
pub mod identity;
pub mod memory;
pub mod messages;
pub mod records;
pub mod session_messages;
pub mod user;
pub mod workspaces;
pub(crate) fn execution_policy(
    client_type: &str,
) -> pontia_core::execution_policy::ExecutionPolicy {
    use pontia_core::execution_policy::ExecutionPolicy;
    let tools = if client_type == "claude-agent" {
        ["Read", "Bash"]
    } else {
        ["read", "bash"]
    };
    ExecutionPolicy {
        tools: Some(tools.into_iter().map(str::to_owned).collect()),
        disable_mcp: true,
        yolo: true,
        ..Default::default()
    }
}
