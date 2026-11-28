use super::ClientAdapter;
use crate::{control::ControlResult, runtime::ControlTarget};
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
                Err(Error::Unsupported(
                    "Manual compaction is unsupported".into(),
                ))
            }
            .await,
        )
    }
}
