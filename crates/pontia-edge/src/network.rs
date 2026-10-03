use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use anyhow::{Context, Result};
use reqwest::{Client, Url};
use serde::Deserialize;

pub fn is_global_unicast(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0 && address.octets()[3] != 9 && address.octets()[3] != 10)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

/// Discover the public egress address; Cloud's subsequent challenge verifies ingress.
pub async fn discover_public_ipv4(cloud_origin: &Url) -> Result<Ipv4Addr> {
    #[derive(Deserialize)]
    struct AddressResponse {
        ipv4: Ipv4Addr,
    }

    let client = Client::builder()
        .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to create IPv4 discovery client")?;
    let response = client
        .get(cloud_origin.join("api/edge/network/address")?)
        .send()
        .await
        .context("failed to discover public IPv4 address through Cloud")?;
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::OK,
        "Cloud IPv4 discovery failed (HTTP {})",
        response.status()
    );
    let address = response
        .json::<AddressResponse>()
        .await
        .context("Cloud returned an invalid IPv4 discovery response")?
        .ipv4;
    anyhow::ensure!(
        is_global_unicast(address),
        "Cloud returned a non-global IPv4 address"
    );
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn discovery_response(status: axum::http::StatusCode, body: &str) -> Result<Ipv4Addr> {
        use axum::{Router, routing::get};

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let origin = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let body = body.to_owned();
        let router = Router::new().route(
            "/api/edge/network/address",
            get(move || async move { (status, [("content-type", "application/json")], body) }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let result = discover_public_ipv4(&origin).await;
        server.abort();
        let _ = server.await;
        result
    }

    #[tokio::test]
    async fn discovers_public_address_not_bound_to_local_interface() {
        let address = discovery_response(axum::http::StatusCode::OK, r#"{"ipv4":"154.8.220.61"}"#)
            .await
            .unwrap();
        assert_eq!(address, "154.8.220.61".parse::<Ipv4Addr>().unwrap());
    }

    #[tokio::test]
    async fn rejects_invalid_discovery_responses() {
        for body in [
            r#"{"ipv4":"10.2.0.10"}"#,
            r#"{"ipv4":"2606:4700::1111"}"#,
            r#"{"ipv4":"invalid"}"#,
            r#"{}"#,
            "not json",
        ] {
            assert!(
                discovery_response(axum::http::StatusCode::OK, body)
                    .await
                    .is_err()
            );
        }
        for status in [
            axum::http::StatusCode::FOUND,
            axum::http::StatusCode::BAD_REQUEST,
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
        ] {
            assert!(
                discovery_response(status, r#"{"ipv4":"154.8.220.61"}"#)
                    .await
                    .is_err()
            );
        }
    }

    #[test]
    fn rejects_non_global_ipv4_ranges() {
        assert!(is_global_unicast("8.8.8.8".parse().unwrap()));
        assert!(is_global_unicast("192.0.0.9".parse().unwrap()));
        assert!(is_global_unicast("192.0.0.10".parse().unwrap()));
        for address in [
            "0.1.2.3",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.2.1",
            "192.168.0.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
        ] {
            assert!(
                !is_global_unicast(address.parse().unwrap()),
                "accepted {address}"
            );
        }
    }
}
