use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use anyhow::{Context, Result};
use nix::ifaddrs::getifaddrs;
use reqwest::Url;

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

pub async fn routed_public_ipv4(website_origin: &Url) -> Result<Ipv4Addr> {
    let host = website_origin
        .host_str()
        .context("Website origin has no hostname")?;
    let website = tokio::net::lookup_host((host, 443))
        .await
        .context("failed to resolve Website hostname")?
        .find(|address| address.is_ipv4())
        .context("Website hostname has no IPv4 address")?;
    routed_public_ipv4_for(website)
}

fn routed_public_ipv4_for(destination: SocketAddr) -> Result<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .context("failed to create IPv4 route socket")?;
    socket
        .connect(destination)
        .context("failed to select a route to Website")?;
    let address = match socket.local_addr()?.ip() {
        IpAddr::V4(address) => address,
        IpAddr::V6(_) => anyhow::bail!("Website route selected IPv6"),
    };
    anyhow::ensure!(
        is_global_unicast(address),
        "Website route did not select a global-unicast IPv4 address"
    );
    let directly_bound = getifaddrs()
        .context("failed to inspect local network interfaces")?
        .filter_map(|interface| interface.address)
        .filter_map(|address| address.as_sockaddr_in().map(|address| address.ip()))
        .any(|candidate| candidate == address);
    anyhow::ensure!(
        directly_bound,
        "selected public IPv4 address is not bound to a local interface"
    );
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;

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
