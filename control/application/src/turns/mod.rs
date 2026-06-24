pub mod claim;
pub mod commands;
mod context;
mod tmux;

pub use claim::{CurrentTurnClaimRequest, CurrentTurnClaimService};
pub use commands::TurnCommandService;
pub(crate) use context::store_client_current_turn_context;

#[cfg(test)]
use serde_json::{Value, json};
#[cfg(test)]
use sqlx::SqlitePool;

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_core::{
        domain::{DomainEvent, EventSource, EventType},
        ids::{new_event_id, new_session_id},
    };

    use crate::EventIngestService;
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};
    use std::{process::Command, time::Duration};

    struct TmuxSessionGuard {
        tmux_session: String,
    }

    impl Drop for TmuxSessionGuard {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &self.tmux_session])
                .status();
        }
    }

    async fn test_pool() -> SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("turn-readiness.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    fn tmux_session_name(session_id: &str) -> String {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        format!("pontia_{sanitized}")
    }

    async fn ingest_session_event(
        service: &EventIngestService,
        session_id: &str,
        event_type: EventType,
        source: EventSource,
        payload: Value,
    ) {
        service
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                source,
                "pi".to_string(),
                event_type,
                payload,
            ))
            .await
            .expect("ingest event");
    }

    #[tokio::test]
    async fn pi_tmux_turn_dispatch_requires_bound_tmux_pane_before_creating_turn() {
        let pool = test_pool().await;
        let session_id = new_session_id().to_string();
        let runtime_instance_id = "rtinst_no_pane";

        let ingest = EventIngestService::new(pool.clone());
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionCreated,
            EventSource::ExternalApi,
            json!({"metadata": {}}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionStarted,
            EventSource::RuntimeManager,
            json!({}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionReady,
            EventSource::AgentClient,
            json!({"runtime_instance_id": runtime_instance_id}),
        )
        .await;

        sqlx::query(
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata) VALUES (?, 'pi_tui', ?, ?)",
        )
        .bind(&session_id)
        .bind(runtime_instance_id)
        .bind(json!({
            "runtime_instance_id": runtime_instance_id,
            "capabilities": {
                "accept_task": true,
                "report_turn_started": true,
                "report_turn_finished": true,
                "interrupt": true,
                "stream_output": true,
                "heartbeat": false,
                "artifact_sources": true
            }
        }).to_string())
        .execute(&pool)
        .await
        .expect("insert runtime binding");

        let error = TurnCommandService::new(pool.clone())
            .create_and_dispatch_turn(&session_id, "cannot web write".to_string(), json!({}))
            .await
            .expect_err("missing pane binding should reject dispatch");
        assert!(
            error.to_string().contains("runtime cannot accept tasks"),
            "unexpected error: {error}"
        );
        let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .expect("turn count");
        assert_eq!(turn_count, 0);
    }

    #[tokio::test]
    async fn pi_tmux_turn_dispatch_waits_for_agent_client_ready() {
        let pool = test_pool().await;
        let session_id = new_session_id().to_string();
        let tmux_session_name = tmux_session_name(&session_id);
        let _guard = TmuxSessionGuard {
            tmux_session: tmux_session_name.clone(),
        };
        let runtime_instance_id = "rtinst_wait_for_ready";

        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &tmux_session_name, "sleep", "30"])
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");
        let socket_path = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &tmux_session_name,
                "#{socket_path}",
            ])
            .output()
            .expect("query socket path");
        assert!(
            socket_path.status.success(),
            "socket path query should succeed"
        );
        let socket_path = String::from_utf8(socket_path.stdout)
            .expect("socket path utf8")
            .trim()
            .to_string();
        let pane_id = Command::new("tmux")
            .args([
                "display-message",
                "-p",
                "-t",
                &tmux_session_name,
                "#{pane_id}",
            ])
            .output()
            .expect("query pane id");
        assert!(pane_id.status.success(), "pane id query should succeed");
        let pane_id = String::from_utf8(pane_id.stdout)
            .expect("pane id utf8")
            .trim()
            .to_string();

        let ingest = EventIngestService::new(pool.clone());
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionCreated,
            EventSource::ExternalApi,
            json!({"metadata": {}}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionStarting,
            EventSource::ExternalApi,
            json!({}),
        )
        .await;
        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionStarted,
            EventSource::RuntimeManager,
            json!({}),
        )
        .await;

        sqlx::query(
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, tmux_socket_path, tmux_pane_id, metadata) VALUES (?, 'tmux', ?, ?, ?, ?)",
        )
        .bind(&session_id)
        .bind(runtime_instance_id)
        .bind(&socket_path)
        .bind(&pane_id)
        .bind(json!({
            "runtime_instance_id": runtime_instance_id,
            "tmux": { "session_name": tmux_session_name },
            "capabilities": {
                "accept_task": true,
                "report_turn_started": true,
                "report_turn_finished": true,
                "interrupt": true,
                "stream_output": true,
                "heartbeat": false,
                "artifact_sources": true
            }
        }).to_string())
        .execute(&pool)
        .await
        .expect("insert runtime binding");

        let service = TurnCommandService::new(pool.clone());
        let dispatch_session_id = session_id.clone();
        let dispatch = tokio::spawn(async move {
            service
                .create_and_dispatch_turn(
                    &dispatch_session_id,
                    "hello after ready".to_string(),
                    json!({}),
                )
                .await
        });

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            !dispatch.is_finished(),
            "pi tmux dispatch must wait for session.ready before completing"
        );

        ingest_session_event(
            &ingest,
            &session_id,
            EventType::SessionReady,
            EventSource::AgentClient,
            json!({"runtime_instance_id": runtime_instance_id}),
        )
        .await;

        tokio::time::timeout(Duration::from_secs(2), dispatch)
            .await
            .expect("dispatch should finish after ready")
            .expect("dispatch task should not panic")
            .expect("dispatch should succeed");
        let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .expect("turn count");
        assert_eq!(
            turn_count, 0,
            "tmux paste dispatch must not create authoritative turn facts before pi hook reports agent_start"
        );
    }
}
