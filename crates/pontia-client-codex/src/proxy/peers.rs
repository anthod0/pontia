use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint, ProcessObservation};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

pub(crate) struct Attachment {
    pub session: String,
    pub runtime_id: String,
    pub process: ProcessFingerprint,
    pub close: watch::Sender<bool>,
}

#[derive(Default)]
pub(crate) struct Peers(Mutex<Vec<Weak<Attachment>>>);

impl Peers {
    pub fn attach(&self, attachment: Attachment) -> Arc<Attachment> {
        let attachment = Arc::new(attachment);
        let mut peers = self.0.lock().unwrap();
        peers.retain(|peer| peer.strong_count() != 0);
        peers.push(Arc::downgrade(&attachment));
        attachment
    }

    pub fn attached(&self, session: &str) -> Vec<Arc<Attachment>> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|peer| peer.session == session)
            .collect()
    }
}

impl Attachment {
    pub async fn terminate(&self) -> Result<()> {
        if self.process.pid == std::process::id() {
            return Err(Error::ControlUnknown(
                "Codex proxy peer is pontiad itself".into(),
            ));
        }
        GenericRuntimeManager.terminate_process(&self.process)?;
        self.close.send_replace(true);
        // The relay drops both sockets before it reports the close to the observer.
        self.close.closed().await;
        if timeout(Duration::from_secs(3), self.wait_exited())
            .await
            .is_err()
        {
            GenericRuntimeManager.kill_process(&self.process)?;
            timeout(Duration::from_secs(3), self.wait_exited())
                .await
                .map_err(|_| Error::ControlUnknown("Codex TUI exit was not confirmed".into()))?;
        }
        Ok(())
    }

    async fn wait_exited(&self) {
        while self.process.observe() != ProcessObservation::Exited {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
