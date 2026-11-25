use std::{io::BufReader, path::Path, time::SystemTime};

use x509_parser::prelude::{FromDer, X509Certificate};

/// Renews once a third of the leaf certificate lifetime remains, or when the file is unusable.
pub fn needs_renewal(path: &Path, now: SystemTime) -> bool {
    let Some((not_before, not_after)) = validity(path) else {
        return true;
    };
    let Ok(now) = now.duration_since(SystemTime::UNIX_EPOCH) else {
        return true;
    };
    renewal_due(not_before, not_after, now.as_secs() as i64)
}

fn validity(path: &Path) -> Option<(i64, i64)> {
    let file = std::fs::File::open(path).ok()?;
    let leaf = rustls_pemfile::certs(&mut BufReader::new(file))
        .next()?
        .ok()?;
    let (_, certificate) = X509Certificate::from_der(&leaf).ok()?;
    let validity = certificate.validity();
    Some((
        validity.not_before.timestamp(),
        validity.not_after.timestamp(),
    ))
}

fn renewal_due(not_before: i64, not_after: i64, now: i64) -> bool {
    if not_after <= not_before || now >= not_after {
        return true;
    }
    let lifetime = not_after - not_before;
    let remaining = not_after - now;
    remaining <= lifetime / 3
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const DAY: i64 = 24 * 60 * 60;

    #[test]
    fn renews_with_one_third_of_the_lifetime_remaining() {
        // 90-day domain certificate: renew from day 60.
        assert!(!renewal_due(0, 90 * DAY, 60 * DAY - 1));
        assert!(renewal_due(0, 90 * DAY, 60 * DAY));
        // 6-day short-lived certificate: renew with two days left.
        assert!(!renewal_due(0, 6 * DAY, 4 * DAY - 1));
        assert!(renewal_due(0, 6 * DAY, 4 * DAY));
        // Expired, inverted, or zero-length validity.
        assert!(renewal_due(0, 90 * DAY, 90 * DAY));
        assert!(renewal_due(90 * DAY, 0, DAY));
        assert!(renewal_due(DAY, DAY, 0));
    }

    #[test]
    fn reads_the_leaf_validity_from_the_saved_pem() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("tls.pem");
        assert!(needs_renewal(&path, SystemTime::now()));
        std::fs::write(&path, "not a certificate").unwrap();
        assert!(needs_renewal(&path, SystemTime::now()));

        let mut params = rcgen::CertificateParams::new(vec!["edge.example.com".into()]).unwrap();
        params.not_before = rcgen::date_time_ymd(2030, 1, 1);
        params.not_after = rcgen::date_time_ymd(2030, 3, 2);
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        std::fs::write(&path, format!("{}{}", cert.pem(), key.serialize_pem())).unwrap();
        let at = |days: u64| {
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_893_456_000 + days * DAY as u64)
        };
        // 2030-01-01 plus 60 days of lifetime: renewal starts after 40 days.
        assert!(!needs_renewal(&path, at(39)));
        assert!(needs_renewal(&path, at(40)));
        assert!(needs_renewal(&path, at(61)));
    }
}
