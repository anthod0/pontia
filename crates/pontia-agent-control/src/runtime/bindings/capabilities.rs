use crate::SessionCapabilities;

pub(crate) fn writable_capabilities(capabilities: &mut SessionCapabilities, writable: bool) {
    capabilities.accept_task = writable;
    capabilities.steer = capabilities.steer && writable;
    capabilities.interrupt = writable;
    capabilities.manual_compaction = capabilities.manual_compaction && writable;
    capabilities.reasoning_effort_control = capabilities.reasoning_effort_control && writable;
    capabilities.fork_session = capabilities.fork_session && writable;
    capabilities.branch_control = capabilities.branch_control && writable;
}
