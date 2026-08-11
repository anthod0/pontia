#[derive(Clone)]
pub struct ShutdownSignal {
    sender: tokio::sync::watch::Sender<bool>,
}

impl ShutdownSignal {
    pub fn notify(&self) {
        let _ = self.sender.send(true);
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.sender.subscribe()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        let (sender, _) = tokio::sync::watch::channel(false);
        Self { sender }
    }
}
