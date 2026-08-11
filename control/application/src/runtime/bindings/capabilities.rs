use crate::SessionCapabilities;

pub(crate) fn writable_capabilities(
    mut capabilities: SessionCapabilities,
    writable: bool,
) -> SessionCapabilities {
    capabilities.accept_task = writable;
    capabilities.interrupt = writable;
    capabilities.branch_control = capabilities.branch_control && writable;
    capabilities
}
