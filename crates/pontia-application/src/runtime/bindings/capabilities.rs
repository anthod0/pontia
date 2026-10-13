use crate::SessionCapabilities;

pub(crate) fn writable_capabilities(
    mut capabilities: SessionCapabilities,
    writable: bool,
) -> SessionCapabilities {
    capabilities.accept_task = writable;
    capabilities.interrupt = writable;
    capabilities.manual_compaction = capabilities.manual_compaction && writable;
    capabilities.branch_control = capabilities.branch_control && writable;
    capabilities
}
