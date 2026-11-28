use crate::{Adapter, Peer, unavailable};
use pontia_agent_control::{
    AgentBindingService, ControlQueryService, EventIngestService, runtime::ControlTarget,
};
use pontia_core::{Error, Result, domain::EventType};
use std::sync::Arc;

pub(super) async fn exit_allows_resume(
    queries: &ControlQueryService,
    session: &str,
    expected_runtime: Option<&str>,
) -> Result<bool> {
    let Some(exit) = queries
        .latest_event(session, EventType::SessionExited)
        .await?
    else {
        return Ok(false);
    };
    let runtime = exit["runtime_id"].as_str();
    if expected_runtime.is_some() && runtime != expected_runtime {
        return Ok(false);
    }
    if exit["reason"] == "claude_sdk_closed" {
        return Ok(true);
    }
    let Some(runtime) = runtime else {
        return Ok(false);
    };
    Ok(queries
        .session_runtime(runtime)
        .await?
        .is_some_and(|row| row.session_id == session && row.state == "exited"))
}

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
            && previous
                .connection()
                .is_ok_and(|connection| !connection.rpc.is_closed())
            && !previous.state.lock().await.closed
        {
            return Ok(());
        }
        if let Some(previous) = peers.get(&target.session_id)
            && previous.connection()?.process.observe()
                != pontia_runtime::ProcessObservation::Exited
        {
            return Err(Error::StateConflict(
                "Claude Code previous execution has not exited".into(),
            ));
        }
        let runtime = target.instance()?;
        let queries = ControlQueryService::new(events.db());
        if !exit_allows_resume(&queries, &target.session_id, Some(runtime)).await? {
            return Err(Error::StateConflict(
                "Claude Code previous execution state is unknown".into(),
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
            runtime: runtime.into(),
            cwd: binding.launch_cwd.into(),
            config_dir,
            model: metadata["launch_model"].as_str().map(str::to_owned),
            resume: Some(binding.client_session_key.clone()),
            native_session_id: Some(binding.client_session_key),
            interactions: self.interactions.get().cloned().ok_or_else(unavailable)?,
            state: Default::default(),
            connection: Default::default(),
        });
        let session_metadata = queries
            .session_metadata(&target.session_id)
            .await?
            .ok_or_else(unavailable)?;
        let environment = queries.runtime_environment(&target.session_id).await?;
        peers.insert(peer.session.clone(), peer.clone());
        drop(peers);
        peer.start(
            &events,
            root,
            &script,
            &environment,
            session_metadata["reasoning_effort"].as_str(),
        )
        .await?;
        Ok(())
    }
}
