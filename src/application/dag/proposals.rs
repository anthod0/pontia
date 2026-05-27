use super::*;

impl DagService {
    pub async fn save_proposal(
        &self,
        task_id: &str,
        payload: &SubmitPlanPayload,
        created_by_session_id: Option<&str>,
    ) -> Result<DagProposal> {
        ensure_task_exists(&self.pool, task_id).await?;
        let proposal_id = new_prefixed_id("dagprop");
        let proposal_json = serde_json::to_string(payload)?;
        let (revision, supersedes_proposal_id) = self.next_proposal_revision(task_id).await?;
        self.supersede_pending_proposals(task_id).await?;
        sqlx::query(
            r#"INSERT INTO dag_proposals (
                    proposal_id, task_id, mode, state, summary, proposal_json,
                    validation_json, created_by_session_id, revision, supersedes_proposal_id
               ) VALUES (?, ?, ?, 'proposed', ?, ?, '{}', ?, ?, ?)"#,
        )
        .bind(&proposal_id)
        .bind(task_id)
        .bind(&payload.mode)
        .bind(&payload.summary)
        .bind(proposal_json)
        .bind(created_by_session_id)
        .bind(revision)
        .bind(supersedes_proposal_id)
        .execute(&self.pool)
        .await?;

        self.get_proposal(&proposal_id).await
    }

    pub async fn save_patch_proposal(
        &self,
        task_id: &str,
        summary: &str,
        patch: &DagPatch,
        created_by_session_id: Option<&str>,
    ) -> Result<DagProposal> {
        ensure_task_exists(&self.pool, task_id).await?;
        let proposal_id = new_prefixed_id("dagprop");
        let proposal_json = serde_json::to_string(&json!({
            "mode": "patch",
            "summary": summary,
            "patch": patch,
        }))?;
        let (revision, supersedes_proposal_id) = self.next_proposal_revision(task_id).await?;
        self.supersede_pending_proposals(task_id).await?;
        sqlx::query(
            r#"INSERT INTO dag_proposals (
                    proposal_id, task_id, mode, state, summary, proposal_json,
                    validation_json, created_by_session_id, revision, supersedes_proposal_id
               ) VALUES (?, ?, 'patch', 'proposed', ?, ?, '{}', ?, ?, ?)"#,
        )
        .bind(&proposal_id)
        .bind(task_id)
        .bind(summary)
        .bind(proposal_json)
        .bind(created_by_session_id)
        .bind(revision)
        .bind(supersedes_proposal_id)
        .execute(&self.pool)
        .await?;

        self.get_proposal(&proposal_id).await
    }

    pub async fn get_proposal(&self, proposal_id: &str) -> Result<DagProposal> {
        let row = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals WHERE proposal_id = ?"#,
        )
        .bind(proposal_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(DagProposal {
            proposal_id: row.get("proposal_id"),
            task_id: row.get("task_id"),
            mode: row.get("mode"),
            state: row.get("state"),
            summary: row.get("summary"),
            proposal_json: parse_json_string(row.get("proposal_json"))?,
            validation_json: parse_json_string(row.get("validation_json"))?,
            created_by_session_id: row.get("created_by_session_id"),
            revision: row.get("revision"),
            supersedes_proposal_id: row.get("supersedes_proposal_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    pub async fn mark_proposal_applied(&self, proposal_id: &str) -> Result<DagProposal> {
        self.mark_proposal_applied_with_result(proposal_id, json!({ "ok": true }))
            .await
    }

    pub async fn mark_proposal_applied_with_result(
        &self,
        proposal_id: &str,
        apply_result: Value,
    ) -> Result<DagProposal> {
        let validation_json = serde_json::to_string(&json!({
            "ok": true,
            "apply_result": apply_result,
        }))?;
        let updated = sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'applied', validation_json = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE proposal_id = ?"#,
        )
        .bind(validation_json)
        .bind(proposal_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(Error::NotFound(format!("proposal {proposal_id}")));
        }
        self.get_proposal(proposal_id).await
    }

    pub async fn mark_proposal_rejected(
        &self,
        proposal_id: &str,
        message: &str,
    ) -> Result<DagProposal> {
        let validation_json = serde_json::to_string(&json!({
            "ok": false,
            "error": message,
        }))?;
        let updated = sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'rejected', validation_json = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE proposal_id = ?"#,
        )
        .bind(validation_json)
        .bind(proposal_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(Error::NotFound(format!("proposal {proposal_id}")));
        }
        self.get_proposal(proposal_id).await
    }

    async fn next_proposal_revision(&self, task_id: &str) -> Result<(i64, Option<String>)> {
        let row: Option<(String, i64)> = sqlx::query_as(
            r#"SELECT proposal_id, revision
               FROM dag_proposals
               WHERE task_id = ?
               ORDER BY revision DESC, created_at DESC, proposal_id DESC
               LIMIT 1"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(match row {
            Some((proposal_id, revision)) => (revision + 1, Some(proposal_id)),
            None => (1, None),
        })
    }

    async fn supersede_pending_proposals(&self, task_id: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'superseded', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ? AND state = 'proposed'"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
