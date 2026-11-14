use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::time::Instant;
use uuid::Uuid;

const LIFETIME: Duration = Duration::from_secs(3 * 60 * 60);

struct Admission {
    device_id: Uuid,
    expires_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn successful_access_renews_three_hours_and_expired_tokens_stay_invalid() {
        let admissions = Admissions::default();
        let device = Uuid::now_v7();
        let token = admissions.issue(device).unwrap();
        tokio::time::advance(Duration::from_secs(2 * 60 * 60)).await;
        assert_eq!(admissions.authenticate(&token), Some(device));
        tokio::time::advance(Duration::from_secs(2 * 60 * 60)).await;
        assert_eq!(admissions.authenticate(&token), Some(device));
        tokio::time::advance(LIFETIME).await;
        assert_eq!(admissions.authenticate(&token), None);
        assert_eq!(admissions.authenticate(&token), None);
    }

    #[tokio::test(start_paused = true)]
    async fn unknown_token_does_not_extend_existing_admissions() {
        let admissions = Admissions::default();
        let device = Uuid::now_v7();
        let token = admissions.issue(device).unwrap();
        tokio::time::advance(Duration::from_secs(2 * 60 * 60)).await;
        assert_eq!(admissions.authenticate("unknown"), None);
        tokio::time::advance(Duration::from_secs(60 * 60)).await;
        assert_eq!(admissions.authenticate(&token), None);
    }

    #[tokio::test(start_paused = true)]
    async fn tokens_are_independent_and_not_restored_in_a_new_edge() {
        let admissions = Admissions::default();
        let device = Uuid::now_v7();
        let first = admissions.issue(device).unwrap();
        let second = admissions.issue(device).unwrap();
        assert_ne!(first, second);
        assert_eq!(admissions.authenticate(&first), Some(device));
        assert_eq!(admissions.authenticate(&second), Some(device));
        assert_eq!(Admissions::default().authenticate(&first), None);
        tokio::task::yield_now().await;
        tokio::time::advance(LIFETIME).await;
        tokio::task::yield_now().await;
        assert_eq!(admissions.authenticate(&first), None);
        assert_eq!(admissions.authenticate(&second), None);
    }
}

#[derive(Clone, Default)]
pub(crate) struct Admissions(Arc<Mutex<HashMap<String, Admission>>>);

impl Admissions {
    pub(crate) fn issue(&self, device_id: Uuid) -> Result<String, getrandom::Error> {
        let mut secret = [0; 32];
        getrandom::fill(&mut secret)?;
        let token = URL_SAFE_NO_PAD.encode(secret);
        let expires_at = Instant::now() + LIFETIME;
        self.0.lock().unwrap().insert(
            token.clone(),
            Admission {
                device_id,
                expires_at,
            },
        );
        let records = Arc::downgrade(&self.0);
        let key = token.clone();
        tokio::spawn(async move {
            let mut deadline = expires_at;
            loop {
                tokio::time::sleep_until(deadline).await;
                let Some(records) = records.upgrade() else {
                    return;
                };
                let mut records = records.lock().unwrap();
                let Some(admission) = records.get(&key) else {
                    return;
                };
                if admission.expires_at <= Instant::now() {
                    records.remove(&key);
                    return;
                }
                deadline = admission.expires_at;
            }
        });
        Ok(token)
    }

    pub(crate) fn authenticate(&self, token: &str) -> Option<Uuid> {
        let mut records = self.0.lock().unwrap();
        let admission = records.get_mut(token)?;
        let now = Instant::now();
        if admission.expires_at <= now {
            records.remove(token);
            return None;
        }
        admission.expires_at = now + LIFETIME;
        Some(admission.device_id)
    }
}
