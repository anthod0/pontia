use crate::{Adapter, Peer, unavailable};
use pontia_application::{AgentBindingService, EventIngestService, runtime::ControlTarget};
use pontia_core::{Error, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

impl Adapter {
    pub(crate) async fn resume_bound(
        &self,
        events: EventIngestService,
        target: &ControlTarget,
    ) -> Result<()> {
        target.validate(&events.db()).await?;
        let mut peers = self.peers.lock().await;
        if self.stopped.load(std::sync::atomic::Ordering::Acquire) {
            return Err(unavailable());
        }
        if let Some(previous) = peers.get(&target.session_id)
            && (!previous.state.lock().await.closed || !*previous.connection()?.exited.borrow())
        {
            return Err(Error::CapabilityUnavailable(
                "Claude Agent previous execution has not confirmed closure".into(),
            ));
        }
        // Runtime exit alone proves only host exit. Require the persisted SDK
        // close confirmation, including when the control service was restarted.
        let payload: Option<String> = sqlx::query_scalar("SELECT payload FROM events WHERE session_id=? AND event_type='session.exited' ORDER BY rowid DESC LIMIT 1")
            .bind(&target.session_id).fetch_optional(&events.db()).await?;
        let payload: Value = payload
            .map(|s| serde_json::from_str(&s))
            .transpose()?
            .unwrap_or(Value::Null);
        if payload["reason"] != "claude_sdk_closed"
            || payload["runtime_id"].as_str() != target.runtime_id.as_deref()
        {
            return Err(Error::CapabilityUnavailable(
                "Claude Agent previous execution state is unknown".into(),
            ));
        }
        let binding = AgentBindingService::new(events.db())
            .binding_for_session(&target.session_id)
            .await?
            .ok_or_else(unavailable)?;
        let metadata = &binding.metadata;
        let config_dir: std::path::PathBuf = metadata["claude_config_dir"]
            .as_str()
            .ok_or_else(unavailable)?
            .into();
        let (_, root) = self.context.get().ok_or_else(unavailable)?;
        let script = self.script(root)?;
        crate::history::read_sdk(
            &script,
            &binding.client_session_key,
            &binding.launch_cwd,
            &config_dir,
        )
        .await?;
        let peer = Arc::new(Peer {
            session: target.session_id.clone(),
            runtime: target.instance()?.into(),
            cwd: binding.launch_cwd.into(),
            config_dir,
            resume: Some(binding.client_session_key),
            interactions: self.interactions.get().cloned().ok_or_else(unavailable)?,
            state: Default::default(),
            connection: OnceLock::new(),
        });
        // Restore the existing Workflow context without persisting secrets or an
        // arbitrary environment snapshot in the binding.
        let workflow: Option<String> =
            sqlx::query_scalar("SELECT workflow_id FROM workflow_nodes WHERE session_id=?")
                .bind(&target.session_id)
                .fetch_optional(&events.db())
                .await?;
        let environment = workflow
            .map(|id| BTreeMap::from([("PONTIA_WORKFLOW_ID".into(), id)]))
            .unwrap_or_default();
        peer.start(&events, root, &script, &environment).await?;
        peers.insert(peer.session.clone(), peer);
        Ok(())
    }
}
