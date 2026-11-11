use super::ClientAdapter;
use crate::{client_contract::DispatchMode, control::ControlResult, runtime::ControlTarget};
use pontia_core::Error;

impl ClientAdapter {
    pub async fn compact(&self, target: &ControlTarget, operation_id: &str) -> ControlResult<()> {
        ControlResult::from_result(
            async {
                target.validate(&self.pool).await?;
                if let Some(client) = self.session_client() {
                    return client
                        .compact(self.events.clone(), target, operation_id)
                        .await;
                }
                match self.spec.adapter.dispatch {
                    DispatchMode::Connected => {
                        self.control
                            .compact(&target.session_id, target.instance()?, operation_id)
                            .await
                    }
                    _ => Err(Error::CapabilityUnavailable(
                        "Manual compaction is unsupported".into(),
                    )),
                }
            }
            .await,
        )
    }
}
