use std::{path::Path, str::FromStr};

use anyhow::{Context, Result};
use axum::http::{HeaderMap, header};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

pub const COOKIE_NAME: &str = "__Host-pontia_edge_access";
const SECRET_PREFIX: &str = "pba_v1_";

#[derive(Clone)]
pub struct BrowserAccess {
    pool: SqlitePool,
}

#[derive(Clone)]
pub(crate) struct BrowserSecret {
    wire: String,
    hash: [u8; 32],
}

impl BrowserSecret {
    pub(crate) fn generate() -> Result<Self> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).context("failed to generate browser capability")?;
        let secret = Self::from_bytes(bytes);
        bytes.fill(0);
        Ok(secret)
    }

    fn from_bytes(bytes: [u8; 32]) -> Self {
        let hash = Sha256::digest(bytes).into();
        Self {
            wire: format!("{SECRET_PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)),
            hash,
        }
    }

    pub(crate) fn parse(wire: &str) -> Option<Self> {
        let encoded = wire.strip_prefix(SECRET_PREFIX)?;
        if encoded.len() != 43
            || !encoded
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return None;
        }
        let bytes: [u8; 32] = URL_SAFE_NO_PAD.decode(encoded).ok()?.try_into().ok()?;
        if URL_SAFE_NO_PAD.encode(bytes) != encoded {
            return None;
        }
        Some(Self::from_bytes(bytes))
    }

    pub(crate) fn wire(&self) -> &str {
        &self.wire
    }
}

impl BrowserAccess {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePool::connect_with(options)
            .await
            .with_context(|| format!("failed to open edge database at {}", path.display()))?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("failed to migrate edge database")?;
        Ok(Self { pool })
    }

    pub async fn authorize(&self, headers: &HeaderMap, device_id: Uuid) -> bool {
        self.authorize_at(headers, device_id, OffsetDateTime::now_utc())
            .await
    }

    pub(crate) async fn authorize_at(
        &self,
        headers: &HeaderMap,
        device_id: Uuid,
        now: OffsetDateTime,
    ) -> bool {
        let Some(secret) = secret_from_headers(headers) else {
            return false;
        };
        self.has_device(&secret, device_id, now).await
    }

    pub(crate) async fn reusable_secret(
        &self,
        headers: &HeaderMap,
        now: OffsetDateTime,
    ) -> Option<BrowserSecret> {
        let secret = secret_from_headers(headers)?;
        self.has_any(&secret, now).await.then_some(secret)
    }

    pub(crate) async fn grant(
        &self,
        secret: &BrowserSecret,
        device_id: Uuid,
        expires_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<OffsetDateTime> {
        let expires_at = canonical_time(expires_at)?;
        let now = canonical_time(now)?;
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO browser_device_access (secret_hash, device_id, expires_at) VALUES (?, ?, ?) \
             ON CONFLICT(secret_hash, device_id) DO UPDATE SET expires_at = excluded.expires_at \
             WHERE browser_device_access.expires_at <= ?",
        )
        .bind(secret.hash.as_slice())
        .bind(device_id.to_string())
        .bind(&expires_at)
        .bind(&now)
        .execute(&mut *transaction)
        .await?;
        let latest: String = sqlx::query_scalar(
            "SELECT MAX(expires_at) FROM browser_device_access \
             WHERE secret_hash = ? AND expires_at > ?",
        )
        .bind(secret.hash.as_slice())
        .bind(&now)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        parse_canonical_time(&latest).context("database contains an invalid capability expiry")
    }

    pub async fn cleanup_expired(&self) -> Result<u64> {
        self.cleanup_expired_at(OffsetDateTime::now_utc()).await
    }

    pub(crate) async fn cleanup_expired_at(&self, now: OffsetDateTime) -> Result<u64> {
        let result = sqlx::query("DELETE FROM browser_device_access WHERE expires_at <= ?")
            .bind(canonical_time(now)?)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    async fn has_any(&self, secret: &BrowserSecret, now: OffsetDateTime) -> bool {
        let Ok(now) = canonical_time(now) else {
            return false;
        };
        sqlx::query(
            "SELECT 1 FROM browser_device_access WHERE secret_hash = ? AND expires_at > ? LIMIT 1",
        )
        .bind(secret.hash.as_slice())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .is_ok_and(|row| row.is_some())
    }

    async fn has_device(
        &self,
        secret: &BrowserSecret,
        device_id: Uuid,
        now: OffsetDateTime,
    ) -> bool {
        let Ok(now) = canonical_time(now) else {
            return false;
        };
        sqlx::query(
            "SELECT 1 FROM browser_device_access \
             WHERE secret_hash = ? AND device_id = ? AND expires_at > ? LIMIT 1",
        )
        .bind(secret.hash.as_slice())
        .bind(device_id.to_string())
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .is_ok_and(|row| row.is_some())
    }
}

fn secret_from_headers(headers: &HeaderMap) -> Option<BrowserSecret> {
    let mut found = None;
    let mut seen = false;
    for value in headers.get_all(header::COOKIE) {
        let value = value.to_str().ok()?;
        for pair in value.split(';') {
            let (name, value) = pair.trim().split_once('=')?;
            if name == COOKIE_NAME {
                if seen {
                    return None;
                }
                seen = true;
                found = BrowserSecret::parse(value);
                found.as_ref()?;
            }
        }
    }
    found
}

pub(crate) fn parse_canonical_time(value: &str) -> Option<OffsetDateTime> {
    if value.len() != 24 || !value.ends_with('Z') || value.as_bytes().get(19) != Some(&b'.') {
        return None;
    }
    let parsed = OffsetDateTime::parse(value, &Rfc3339).ok()?;
    (canonical_time(parsed).ok()?.as_str() == value).then_some(parsed)
}

pub(crate) fn canonical_time(value: OffsetDateTime) -> Result<String> {
    let milliseconds = value.unix_timestamp_nanos().div_euclid(1_000_000);
    let seconds = milliseconds.div_euclid(1_000);
    let millis = milliseconds.rem_euclid(1_000);
    let utc = OffsetDateTime::from_unix_timestamp(seconds as i64)?;
    Ok(format!(
        "{}.{millis:03}Z",
        utc.format(&time::format_description::parse(
            "[year]-[month]-[day]T[hour]:[minute]:[second]"
        )?)?
    ))
}

pub(crate) fn canonical_uuid(value: &str) -> Option<Uuid> {
    let id = Uuid::from_str(value).ok()?;
    (id.to_string() == value).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(secret: &BrowserSecret) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{COOKIE_NAME}={}", secret.wire()).parse().unwrap(),
        );
        headers
    }

    #[tokio::test]
    async fn capabilities_persist_and_are_scoped_by_secret_and_device() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let path = test_root.path().join("edge.sqlite3");
        let access = BrowserAccess::open(&path).await.unwrap();
        let now = OffsetDateTime::from_unix_timestamp(2_000_000_000).unwrap();
        let expiry = now + time::Duration::days(30);
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let unauthorized = Uuid::new_v4();
        let shared = BrowserSecret::generate().unwrap();
        let isolated = BrowserSecret::generate().unwrap();

        access.grant(&shared, first, expiry, now).await.unwrap();
        assert_eq!(
            access
                .grant(&shared, first, expiry + time::Duration::days(1), now)
                .await
                .unwrap(),
            expiry
        );
        assert_eq!(
            access
                .grant(&shared, second, expiry + time::Duration::days(2), now)
                .await
                .unwrap(),
            expiry + time::Duration::days(2)
        );
        access
            .grant(&isolated, unauthorized, expiry, now)
            .await
            .unwrap();
        drop(access);

        let reopened = BrowserAccess::open(&path).await.unwrap();
        assert!(reopened.authorize_at(&cookie(&shared), first, now).await);
        assert!(reopened.authorize_at(&cookie(&shared), second, now).await);
        assert!(
            !reopened
                .authorize_at(&cookie(&shared), unauthorized, now)
                .await
        );
        assert!(!reopened.authorize_at(&cookie(&isolated), first, now).await);
        assert!(!reopened.authorize_at(&HeaderMap::new(), first, now).await);
        assert!(
            !reopened
                .authorize_at(&cookie(&shared), first, expiry + time::Duration::seconds(1),)
                .await
        );

        let bytes = std::fs::read(path).unwrap();
        assert!(
            !bytes
                .windows(shared.wire().len())
                .any(|part| part == shared.wire().as_bytes())
        );
    }

    #[tokio::test]
    async fn expired_capabilities_fail_before_and_after_cleanup() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let access = BrowserAccess::open(&test_root.path().join("edge.sqlite3"))
            .await
            .unwrap();
        let now = OffsetDateTime::from_unix_timestamp(2_000_000_000).unwrap();
        let secret = BrowserSecret::generate().unwrap();
        let device = Uuid::new_v4();
        access
            .grant(&secret, device, now + time::Duration::seconds(1), now)
            .await
            .unwrap();

        let expired = now + time::Duration::seconds(2);
        assert!(!access.authorize_at(&cookie(&secret), device, expired).await);
        assert_eq!(access.cleanup_expired_at(expired).await.unwrap(), 1);
        assert!(!access.authorize_at(&cookie(&secret), device, expired).await);
    }

    #[tokio::test]
    async fn database_initialization_fails_closed() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let blocking_file = test_root.path().join("not-a-directory");
        std::fs::write(&blocking_file, b"file").unwrap();
        assert!(
            BrowserAccess::open(&blocking_file.join("edge.sqlite3"))
                .await
                .is_err()
        );

        let invalid_database = test_root.path().join("invalid.sqlite3");
        std::fs::write(&invalid_database, b"not sqlite").unwrap();
        assert!(BrowserAccess::open(&invalid_database).await.is_err());
    }

    #[test]
    fn browser_secret_and_time_parsing_are_canonical() {
        let secret = BrowserSecret::generate().unwrap();
        assert_eq!(secret.wire().len(), 50);
        assert!(BrowserSecret::parse(secret.wire()).is_some());
        for invalid in [
            secret.wire().replace("pba_v1_", "pet_v1_"),
            format!("{}=", secret.wire()),
            format!("{}x", secret.wire()),
        ] {
            assert!(BrowserSecret::parse(&invalid).is_none());
        }
        assert!(parse_canonical_time("2030-01-01T00:00:00.000Z").is_some());
        assert!(parse_canonical_time("2030-01-01T00:00:00Z").is_none());
        assert!(parse_canonical_time("2030-01-01T00:00:00.000+00:00").is_none());
    }
}
