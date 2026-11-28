use super::SqliteSessionRepository;
use pontia_core::{Error, Result};
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, Transaction};

fn key_path(key: &str) -> Result<String> {
    Ok(format!("$.{}", serde_json::to_string(key)?))
}

async fn write_metadata(
    tx: &mut Transaction<'_, Sqlite>,
    session: &str,
    mut update: QueryBuilder<'_, Sqlite>,
) -> Result<()> {
    let result = update
        .push(") WHERE session_id=")
        .push_bind(session.to_owned())
        .build()
        .execute(&mut **tx)
        .await?;
    if result.rows_affected() == 0 {
        return Err(Error::NotFound(format!("Session {session}")));
    }
    Ok(())
}

impl SqliteSessionRepository {
    pub async fn patch_metadata_keys_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        patches: &[(&str, Value)],
    ) -> Result<()> {
        if patches.is_empty() {
            return Ok(());
        }
        let mut update = QueryBuilder::new("UPDATE sessions SET metadata=json_set(metadata");
        for (key, value) in patches {
            update
                .push(",")
                .push_bind(key_path(key)?)
                .push(",json(")
                .push_bind(value.to_string())
                .push(")");
        }
        write_metadata(tx, session, update).await
    }

    pub async fn remove_metadata_keys_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        keys: &[&str],
    ) -> Result<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut update = QueryBuilder::new("UPDATE sessions SET metadata=json_remove(metadata");
        for key in keys {
            update.push(",").push_bind(key_path(key)?);
        }
        write_metadata(tx, session, update).await
    }

    pub async fn update_metadata_key_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        key: &str,
        update: impl FnOnce(Option<Value>) -> Result<Value>,
    ) -> Result<()> {
        let previous = sqlx::query_scalar::<_, Option<Value>>(
            "SELECT metadata -> ? FROM sessions WHERE session_id=?",
        )
        .bind(key_path(key)?)
        .bind(session)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Session {session}")))?;
        Self::patch_metadata_keys_in_tx(tx, session, &[(key, update(previous)?)]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn multi_key_patch_has_one_revision_and_rolls_back_with_the_caller() {
        let pool = crate::connect_sqlite("sqlite::memory:").await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO sessions(session_id,client_type,state) VALUES ('s','client','idle')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let revision = "SELECT revision FROM control_state_revision WHERE id=1";
        let before: i64 = sqlx::query_scalar(revision)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        SqliteSessionRepository::patch_metadata_keys_in_tx(
            &mut tx,
            "s",
            &[("first", json!(false)), ("second", json!({"n":1}))],
        )
        .await
        .unwrap();
        let after: i64 = sqlx::query_scalar(revision)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(after, before + 1);
        tx.rollback().await.unwrap();
        assert_eq!(
            SqliteSessionRepository::new(pool.clone())
                .get_metadata("s")
                .await
                .unwrap(),
            Some(json!({}))
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(revision)
                .fetch_one(&pool)
                .await
                .unwrap(),
            before
        );
    }
}
