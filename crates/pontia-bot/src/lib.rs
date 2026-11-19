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
pub(crate) fn execution_policy() -> pontia_core::execution_policy::ExecutionPolicy {
    use pontia_core::execution_policy::{ExecutionPolicy, ExecutionTool};
    ExecutionPolicy {
        tools: Some(vec![ExecutionTool::Read, ExecutionTool::Bash]),
        yolo: true,
    }
}
