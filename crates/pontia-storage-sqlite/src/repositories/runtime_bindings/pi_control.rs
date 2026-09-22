use pontia_core::{Error, Result};

use super::SqliteRuntimeBindingRepository;

#[derive(sqlx::FromRow)]
pub struct PiControlBindingRow {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub endpoint: String,
}

impl SqliteRuntimeBindingRepository {
    pub async fn publish_pi_control_endpoint(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        endpoint: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE runtime_bindings
               SET adapter_details = json_set(adapter_details, '$.pi_control', json(?))
               WHERE session_id = ? AND runtime_instance_id = ? AND binding_state = 'confirmed'
                 AND EXISTS (SELECT 1 FROM sessions s WHERE s.session_id = runtime_bindings.session_id
                             AND s.client_type = 'pi' AND s.state IN ('starting', 'idle', 'busy', 'interrupted'))"#,
        ).bind(endpoint).bind(session_id).bind(runtime_instance_id).execute(&self.pool).await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "Pi control endpoint does not match a current confirmed running instance".into(),
            ));
        }
        Ok(())
    }

    pub async fn pi_control_bindings(&self) -> Result<Vec<PiControlBindingRow>> {
        Ok(sqlx::query_as(
            r#"SELECT r.session_id, r.runtime_instance_id,
                      json_extract(r.adapter_details, '$.pi_control') AS endpoint
               FROM runtime_bindings r JOIN sessions s ON s.session_id = r.session_id
               WHERE s.client_type = 'pi' AND s.state IN ('starting', 'idle', 'busy', 'interrupted')
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id IS NOT NULL
                 AND json_type(r.adapter_details, '$.pi_control') = 'object'
                 AND json_extract(r.adapter_details, '$.pi_control.runtime_instance_id') = r.runtime_instance_id"#,
        ).fetch_all(&self.pool).await?)
    }
}
