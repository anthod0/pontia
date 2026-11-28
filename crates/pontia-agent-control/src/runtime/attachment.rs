use pontia_core::{Error, Result};
use pontia_runtime::ProcessFingerprint;
use pontia_storage_sqlite::repositories::{
    session_runtimes::SqliteSessionRuntimeRepository as Runtimes, turns::SqliteTurnRepository,
};
use sqlx::SqlitePool;

pub enum AttachmentBinding<'a> {
    Launch,
    Native(&'a str),
    /// The client verified a pending ready report for this native key.
    PendingReady(&'a str),
}

pub enum AttachmentProcess<'a> {
    Confirmed(&'a ProcessFingerprint),
    BackfillIfStarting(&'a ProcessFingerprint),
    /// In-process channels have no OS peer.
    InProcess,
}

#[derive(Clone)]
pub struct RuntimeAttachmentService {
    pool: SqlitePool,
}

impl RuntimeAttachmentService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn confirm_attachment(
        &self,
        session: &str,
        runtime: &str,
        client: &str,
        role: &str,
        binding: AttachmentBinding<'_>,
        process: AttachmentProcess<'_>,
    ) -> Result<()> {
        if matches!(process, AttachmentProcess::InProcess)
            && !matches!(binding, AttachmentBinding::Native(_))
        {
            return Err(Error::StateConflict(
                "An in-process attachment requires a native binding".into(),
            ));
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        SqliteTurnRepository::serialize_session_turn_writes_in_tx(&mut tx, session).await?;
        let row = Runtimes::active_attachment(&mut tx, session, role)
            .await?
            .filter(|r| r.runtime_id == runtime && r.client_type == client)
            .ok_or_else(|| {
                Error::StateConflict("Attachment does not match an active control runtime".into())
            })?;
        // Launch and native attachments must refer to the current control target.
        if Runtimes::runtime_id_in_tx(&mut tx, session, Some(role))
            .await?
            .as_deref()
            != Some(runtime)
        {
            return Err(Error::StateConflict("Control runtime changed".into()));
        }
        match binding {
            AttachmentBinding::Launch if row.runtime_state != "starting" => {
                return Err(Error::StateConflict(
                    "Runtime must confirm its native binding".into(),
                ));
            }
            AttachmentBinding::Launch => {}
            AttachmentBinding::Native(key) | AttachmentBinding::PendingReady(key) => {
                let bound: Option<String> = sqlx::query_scalar(
                    "SELECT session_id FROM agent_bindings WHERE client_type=? AND client_session_key=?",
                )
                .bind(client)
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?;
                if bound.is_none() && matches!(binding, AttachmentBinding::Native(_)) {
                    return Err(Error::NotFound("Native binding not found".into()));
                }
                if bound.as_deref() != Some(session) {
                    let pending = matches!(binding, AttachmentBinding::PendingReady(_))
                        && Runtimes::pending_ready_fingerprint(
                            &mut tx, session, runtime, client, key,
                        )
                        .await?
                        .is_some();
                    if !pending {
                        return Err(Error::StateConflict(
                            "Native binding does not match runtime identity".into(),
                        ));
                    }
                }
            }
        }
        match process {
            AttachmentProcess::BackfillIfStarting(peer)
                if row.runtime_state == "starting" && row.process_fingerprint.is_none() =>
            {
                Runtimes::backfill_process_fingerprint_in_tx(
                    &mut tx,
                    session,
                    runtime,
                    &serde_json::to_string(peer)?,
                )
                .await?;
            }
            AttachmentProcess::Confirmed(peer) | AttachmentProcess::BackfillIfStarting(peer) => {
                let confirmed = row
                    .process_fingerprint
                    .as_deref()
                    .map(serde_json::from_str::<ProcessFingerprint>)
                    .transpose()?;
                if confirmed.as_ref() != Some(peer) {
                    return Err(Error::StateConflict(
                        "Control runtime process does not match".into(),
                    ));
                }
            }
            AttachmentProcess::InProcess => {}
        }
        sqlx::query(
            "UPDATE sessions SET metadata=json_remove(metadata,'$.incompatible_control_runtime')
            WHERE session_id=? AND json_type(metadata,'$.incompatible_control_runtime') IS NOT NULL",
        )
            .bind(session)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn pending_ready_fingerprint(
        &self,
        session: &str,
        runtime: &str,
        client: &str,
        key: &str,
    ) -> Result<Option<ProcessFingerprint>> {
        let mut conn = self.pool.acquire().await?;
        Ok(
            Runtimes::pending_ready_fingerprint(&mut conn, session, runtime, client, key)
                .await?
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
        )
    }

    pub async fn active_control_runtime_id(
        &self,
        session: &str,
        role: &str,
    ) -> Result<Option<String>> {
        Runtimes::new(self.pool.clone())
            .active_control_runtime_id(session, role)
            .await
    }

    pub async fn list_active_by_role(&self, role: &str) -> Result<Vec<(String, String)>> {
        Runtimes::new(self.pool.clone())
            .list_active_by_role(role)
            .await
    }

    pub async fn count_running_by_role(&self, session: &str, role: &str) -> Result<i64> {
        Runtimes::new(self.pool.clone())
            .count_running_by_role(session, role)
            .await
    }

    pub async fn session_exited_with_runtime(&self, session: &str, runtime: &str) -> Result<bool> {
        Runtimes::new(self.pool.clone())
            .session_exited_with_runtime(session, runtime)
            .await
    }
}
