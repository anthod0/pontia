use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    time::Duration,
};

use anyhow::{Context, Result};
use hickory_resolver::{TokioResolver, config::LookupIpStrategy};
use pontia_tunnel::endpoint::is_public_ip;
use socket2::{Domain, Protocol, Socket, Type};

/// Use the DNS servers configured by the OS, without pinning an external provider.
pub fn system_dns_resolver() -> Result<TokioResolver> {
    let mut builder = TokioResolver::builder_tokio()
        .context("failed to load system DNS resolver configuration")?;
    // Newly published records must not be hidden by our own negative cache.
    builder.options_mut().cache_size = 0;
    // Propagation must be observed in DNS, not satisfied by a local /etc/hosts entry.
    builder.options_mut().use_hosts_file = hickory_resolver::config::ResolveHosts::Never;
    builder.options_mut().timeout = Duration::from_secs(5);
    builder.options_mut().attempts = 1;
    builder.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
    Ok(builder.build())
}

pub fn is_global_unicast(address: IpAddr) -> bool {
    is_public_ip(&address.to_string())
}

/// Checks the public DNS records for an edge hostname before requesting a certificate.
pub async fn public_addresses(hostname: &str) -> Result<Vec<IpAddr>> {
    if let Ok(address) = hostname.parse::<IpAddr>() {
        anyhow::ensure!(
            is_global_unicast(address),
            "{hostname} is not a public unicast IP address"
        );
        return Ok(vec![address]);
    }
    let resolver = system_dns_resolver()?;
    let lookup = tokio::time::timeout(
        Duration::from_secs(10),
        resolver.lookup_ip(format!("{hostname}.")),
    )
    .await
    .with_context(|| format!("system DNS lookup for {hostname} timed out"))?
    .with_context(|| format!("{hostname} has no A or AAAA record in public DNS"))?;
    let addresses: Vec<IpAddr> = lookup.iter().collect();
    anyhow::ensure!(
        !addresses.is_empty(),
        "{hostname} has no A or AAAA record in public DNS"
    );
    anyhow::ensure!(
        addresses.iter().copied().any(is_global_unicast),
        "{hostname} resolves only to non-public addresses ({})",
        addresses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(addresses)
}

/// Listens on IPv4 and IPv6, or on IPv4 only when the host has no IPv6 support.
pub fn bind_listener(port: u16) -> Result<TcpListener> {
    let listener = match bind_socket(SocketAddr::from((Ipv6Addr::UNSPECIFIED, port))) {
        Err(error) if ipv6_unavailable(&error) => {
            bind_socket(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)))
        }
        result => result,
    };
    listener.with_context(|| format!("failed to listen on port {port}"))
}

fn bind_socket(address: SocketAddr) -> io::Result<TcpListener> {
    let socket = Socket::new(
        Domain::for_address(address),
        Type::STREAM,
        Some(Protocol::TCP),
    )?;
    if address.is_ipv6() {
        socket.set_only_v6(false)?;
    }
    socket.set_reuse_address(true)?;
    socket.bind(&address.into())?;
    socket.listen(1024)?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

fn ipv6_unavailable(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == rustix::io::Errno::AFNOSUPPORT.raw_os_error()
            || code == rustix::io::Errno::ADDRNOTAVAIL.raw_os_error()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_global_ipv4_ranges() {
        for address in ["8.8.8.8", "192.0.0.9", "192.0.0.10", "154.8.220.61"] {
            assert!(is_global_unicast(address.parse().unwrap()), "{address}");
        }
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
            "255.255.255.255",
        ] {
            assert!(
                !is_global_unicast(address.parse().unwrap()),
                "accepted {address}"
            );
        }
    }

    #[test]
    fn accepts_only_allocated_global_ipv6_unicast() {
        for address in [
            "2606:4700:4700::1111",
            "2a00:1450:4001::200e",
            "2400:cb00::1",
        ] {
            assert!(is_global_unicast(address.parse().unwrap()), "{address}");
        }
        for address in [
            "::",
            "::1",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "100::1",
            "2001::1",
            "2001:db8::1",
            "2002:c000:0201::1",
            "3fff::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "fec0::1",
            "ff02::1",
        ] {
            assert!(
                !is_global_unicast(address.parse().unwrap()),
                "accepted {address}"
            );
        }
    }

    #[tokio::test]
    async fn listener_accepts_ipv4_clients_on_the_shared_socket() {
        let listener = tokio::net::TcpListener::from_std(bind_listener(0).unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();
        let accept = tokio::spawn(async move { listener.accept().await.map(|_| ()) });
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        accept.await.unwrap().unwrap();
    }
}
