mod native_bindings;
pub(crate) use native_bindings::NativeRuntimeBindings;
mod records;
pub mod shutdown;
pub use control_target::ControlTarget;
pub(crate) use records::{runtime_binding_record, terminate_runtime};
pub mod bindings;
mod control_target;
mod observation;
mod readiness;

pub use bindings::{RuntimeBindingUpsertRequest, RuntimeBindingUpsertService};
pub use observation::RuntimeObservationService;
pub use readiness::RuntimeReadinessService;

mod attachment;
pub use attachment::{AttachmentBinding, AttachmentProcess, RuntimeAttachmentService};
