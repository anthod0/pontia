use super::ClientOperation;
use crate::{EventIngestService, interactions::InteractionResponse};

/// Resolves an interaction using a client-owned, connection-bound native association.
/// Success means the native response has been written, not merely queued.
pub trait ClientInteractions: Send + Sync {
    fn resolve<'a>(
        &'a self,
        events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        response: &'a InteractionResponse,
    ) -> ClientOperation<'a, ()>;
}
