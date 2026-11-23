use super::Shutdown;
use serde::{Deserialize, Serialize};
use tokio_stream::Stream;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Down,
    Uninstall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShutdownRequest {
    pub pontia_home: std::path::PathBuf,
    pub operation: Operation,
    pub force: bool,
    pub stop: bool,
    pub lock_receiver: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownUpdate {
    Listing(Vec<String>),
    Stopped(Vec<String>),
    Failed(String),
}

impl Shutdown {
    pub fn execute(
        self,
        request: ShutdownRequest,
    ) -> Result<impl Stream<Item = ShutdownUpdate>, String> {
        let listing = self.listing();
        if !request.force && !listing.is_empty() {
            let operation = match request.operation {
                Operation::Down => "down",
                Operation::Uninstall => "uninstall",
            };
            return Err(format!(
                "{operation} refused while agent processes are running:\n  - {}\nRun `pontia {operation} --force` to stop Pontia-started agents and continue.",
                listing.join("\n  - ")
            ));
        }
        let receiver = request
            .lock_receiver
            .map(pontia_runtime::launch_lock::connect_receiver)
            .transpose()
            .map_err(|error| error.to_string())?;
        let prepared = request.stop.then_some(self);
        Ok(async_stream::stream! {
            yield ShutdownUpdate::Listing(listing);
            if let Some(mut shutdown) = prepared {
                let result = shutdown.terminate().await.and_then(|warnings| {
                    if let Some(socket) = receiver {
                        shutdown.lock.hand_off(&socket)?;
                    }
                    Ok(warnings)
                });
                yield match result {
                    Ok(warnings) => ShutdownUpdate::Stopped(warnings),
                    Err(error) => ShutdownUpdate::Failed(error.to_string()),
                };
            }
        })
    }
}
