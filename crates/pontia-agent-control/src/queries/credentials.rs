use super::ControlQueryService;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use pontia_core::Result;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::LazyLock};
use subtle::ConstantTimeEq;

// A daemon can recreate its launch credentials without keeping plaintext in SQLite.
fn launch_credential(session: &str) -> String {
    static KEY: LazyLock<uuid::Uuid> = LazyLock::new(uuid::Uuid::new_v4);
    let mut mac = Hmac::<Sha256>::new_from_slice(KEY.as_bytes()).expect("valid HMAC key");
    mac.update(session.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

impl ControlQueryService {
    pub async fn runtime_environment(&self, session: &str) -> Result<BTreeMap<String, String>> {
        let (stored, key, digest): (String, Option<String>, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT runtime_environment, credential_env, credential_sha256 FROM sessions WHERE session_id=?"
        ).bind(session).fetch_one(&self.pool).await?;
        let mut environment: BTreeMap<String, String> = serde_json::from_str(&stored)?;
        if let Some(key) = key {
            let token = launch_credential(session);
            let current = Sha256::digest(&token);
            // After a daemon restart the running client keeps its credential until relaunch.
            if digest.as_deref() != Some(&current[..]) {
                sqlx::query("UPDATE sessions SET credential_sha256=? WHERE session_id=?")
                    .bind(&current[..])
                    .bind(session)
                    .execute(&self.pool)
                    .await?;
            }
            environment.insert(key, token);
        }
        Ok(environment)
    }

    pub async fn verify_session_credential(
        &self,
        session: &str,
        key: &str,
        credential: &str,
    ) -> Result<bool> {
        let digest: Option<Vec<u8>> = sqlx::query_scalar("SELECT credential_sha256 FROM sessions WHERE session_id=? AND credential_env=? AND credential_sha256 IS NOT NULL")
            .bind(session).bind(key).fetch_optional(&self.pool).await?;
        Ok(digest.is_some_and(|expected| expected.ct_eq(&Sha256::digest(credential)).into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relaunch_replaces_previous_daemon_credential_and_persists_only_its_digest() {
        let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO sessions(session_id,client_type,state,runtime_environment,credential_env,credential_sha256) VALUES ('s','generic','idle','{"KEEP":"value"}','TOKEN',?)"#)
            .bind(Sha256::digest("previous-daemon-token").to_vec()).execute(&pool).await.unwrap();
        let queries = ControlQueryService::new(pool.clone());
        assert!(
            queries
                .verify_session_credential("s", "TOKEN", "previous-daemon-token")
                .await
                .unwrap()
        );
        let environment = queries.runtime_environment("s").await.unwrap();
        assert_eq!(environment["KEEP"], "value");
        let token = &environment["TOKEN"];
        assert!(
            queries
                .verify_session_credential("s", "TOKEN", token)
                .await
                .unwrap()
        );
        assert!(
            !queries
                .verify_session_credential("s", "TOKEN", "previous-daemon-token")
                .await
                .unwrap()
        );
        let (stored, digest): (String, Vec<u8>) = sqlx::query_as(
            "SELECT runtime_environment,credential_sha256 FROM sessions WHERE session_id='s'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored).unwrap(),
            serde_json::json!({"KEEP":"value"})
        );
        assert_eq!(digest, Sha256::digest(token).as_slice());
    }
}
