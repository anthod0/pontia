use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlChange {
    WorkspaceRegistered {
        workspace_id: String,
        name: Option<String>,
        canonical_path: String,
        description: Option<String>,
        source_session_id: Option<String>,
    },
    AgentProfileCreated {
        source_session_id: Option<String>,
        profile_id: String,
        name: String,
        description: Option<String>,
    },
    AgentProfileDeleted {
        source_session_id: Option<String>,
        profile_id: String,
    },
    AgentProfileDescriptionChanged {
        source_session_id: Option<String>,
        profile_id: String,
        description: Option<String>,
    },
}

#[derive(Clone)]
pub struct ControlChanges(broadcast::Sender<ControlChange>);

impl Default for ControlChanges {
    fn default() -> Self {
        Self(broadcast::channel(1024).0)
    }
}

impl ControlChanges {
    pub fn subscribe(&self) -> broadcast::Receiver<ControlChange> {
        self.0.subscribe()
    }

    pub(crate) fn publish(&self, change: ControlChange) {
        let _ = self.0.send(change);
    }
}
