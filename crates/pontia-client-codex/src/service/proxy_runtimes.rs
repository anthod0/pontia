use super::CodexService;
use crate::{
    proxy::unsubscribe_confirmed,
    runtime::{CodexRuntime, SubscriptionState},
};
use pontia_agent_control::{AgentBindingService, PontiaEvent, PontiaEventSource, PontiaEventType};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
use serde_json::json;

const ROLE: &str = "codex_proxy";

impl CodexService {
    // The caller retains the Session operation lock from adoption through registration.
    pub(super) async fn register_proxy_runtime(
        &self,
        session: &str,
        runtime_id: &str,
    ) -> Result<()> {
        if SqliteSessionRuntimeRepository::new(self.pool.clone())
            .get(runtime_id)
            .await?
            .is_some_and(|record| record.state == "running")
        {
            return Ok(());
        }
        for event_type in [
            PontiaEventType::RuntimeStarting,
            PontiaEventType::RuntimeReady,
        ] {
            self.proxy_runtime_fact(session, runtime_id, event_type)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn end_proxy_runtime(
        &self,
        runtime: &CodexRuntime,
        thread: &str,
        runtime_id: &str,
    ) -> Result<()> {
        let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("codex", thread)
            .await?
        else {
            return Ok(());
        };
        let session = &binding.session_id;
        let _operation = runtime.lock_session(session).await;
        let Some(record) = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .get(runtime_id)
            .await?
        else {
            return Ok(());
        };
        if record.state != "running" {
            return Ok(());
        }
        self.proxy_runtime_fact(session, runtime_id, PontiaEventType::RuntimeExited)
            .await?;
        self.finish_proxy_management(runtime, session, thread).await
    }

    pub(super) async fn archive_proxy_runtimes(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
    ) -> Result<()> {
        let _operation = runtime.lock_session(session).await;
        self.reconcile_proxy_archive(runtime, session, thread, None)
            .await
    }

    // Adoption drains earlier archives under the same lock before registering a new association.
    pub(super) async fn reconcile_proxy_archive(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
        boundary: Option<u64>,
    ) -> Result<()> {
        {
            let connection = runtime.connection().await?;
            let mut pending = connection.pending_archives.lock().await;
            if let Some(sequence) = pending.get(thread)
                && boundary.is_some_and(|boundary| *sequence > boundary)
            {
                return Err(Error::CapabilityUnavailable(
                    "Codex thread was archived before attachment".into(),
                ));
            }
            if pending.remove(thread).is_none() {
                return Ok(());
            }
        }
        let records = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .list(session)
            .await?;
        let mut ended = false;
        for record in records
            .into_iter()
            .filter(|record| record.role == ROLE && record.state == "running")
        {
            self.proxy_runtime_fact(session, &record.runtime_id, PontiaEventType::RuntimeExited)
                .await?;
            ended = true;
        }
        if ended {
            self.finish_proxy_management(runtime, session, thread)
                .await?;
        }
        Ok(())
    }

    async fn proxy_runtime_fact(
        &self,
        session: &str,
        runtime_id: &str,
        event_type: PontiaEventType,
    ) -> Result<()> {
        self.event_ingest
            .ingest_pontia_event(PontiaEvent::new(
                session,
                None,
                PontiaEventSource::RuntimeManager,
                "codex",
                event_type,
                json!({"runtime_id":runtime_id,"role":ROLE}),
            ))
            .await?;
        Ok(())
    }

    async fn finish_proxy_management(
        &self,
        runtime: &CodexRuntime,
        session: &str,
        thread: &str,
    ) -> Result<()> {
        let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes WHERE session_id=? AND role=? AND state='running'")
            .bind(session).bind(ROLE).fetch_one(&self.pool).await?;
        if active != 0
            || runtime.subscription(session).await == Some(SubscriptionState::ExitPending)
            || Self::has_confirmed_exit(session).await
        {
            return Ok(());
        }
        let exited: bool =
            sqlx::query_scalar("SELECT state='exited' FROM sessions WHERE session_id=?")
                .bind(session)
                .fetch_one(&self.pool)
                .await?;
        if exited {
            return Ok(());
        }
        let result = runtime
            .connection()
            .await?
            .call("thread/unsubscribe", json!({"threadId":thread}))
            .await?;
        if !unsubscribe_confirmed(&result) {
            return Err(Error::ControlUnknown(
                "Codex unsubscribe did not confirm subscription removal".into(),
            ));
        }
        let _current = runtime.current_guard().await?;
        self.suspend_reload(session).await?;
        self.confirm_exit(session, runtime).await;
        self.event_ingest
            .ingest_pontia_event(PontiaEvent::new(
                session,
                None,
                PontiaEventSource::RuntimeManager,
                "codex",
                PontiaEventType::SessionExited,
                json!({"reason":"proxy_subscriptions_ended"}),
            ))
            .await?;
        Ok(())
    }
}
