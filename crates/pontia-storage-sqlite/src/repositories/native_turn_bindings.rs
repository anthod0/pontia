use pontia_core::Result;
use sqlx::SqlitePool;

pub struct SqliteNativeTurnBindingRepository {
    pool: SqlitePool,
}

impl SqliteNativeTurnBindingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn client_turn_id(&self, session_id: &str, turn_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?",
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn turn_id(&self, session_id: &str, client_turn_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
        )
        .bind(session_id)
        .bind(client_turn_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn exists(&self, session_id: &str, client_turn_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?)",
        )
            .bind(session_id)
            .bind(client_turn_id)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn running_turn_id(
        &self,
        session_id: &str,
        client_turn_id: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT b.turn_id FROM native_turn_bindings b JOIN turns t USING(session_id,turn_id) WHERE b.session_id=? AND b.client_turn_id=? AND t.state='running'",
        )
            .bind(session_id)
            .bind(client_turn_id)
            .fetch_optional(&self.pool)
            .await?)
    }
}
