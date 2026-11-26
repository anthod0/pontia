//! Public edge endpoint rules shared by devices and edges.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Fetch Standard bad ports, plus ports the control-plane runtime cannot reach.
pub const UNSAFE_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
    6697, 10080,
];

// Local, private, and special-use names (RFC 6761, RFC 6762, RFC 8375, RFC 9476).
const RESERVED_TOP_LEVEL_LABELS: &[&str] = &[
    "localhost",
    "local",
    "internal",
    "arpa",
    "localdomain",
    "invalid",
    "onion",
    "alt",
];

pub fn is_safe_port(port: u16) -> bool {
    port != 0 && !UNSAFE_PORTS.contains(&port)
}

/// A canonical lowercase public DNS name with at least two labels and no IP literal.
pub fn is_public_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 253 {
        return false;
    }
    let labels: Vec<&str> = value.split('.').collect();
    let Some(top_level) = labels.last() else {
        return false;
    };
    labels.len() >= 2
        && labels.iter().all(|label| valid_label(label))
        && valid_top_level_label(top_level)
        && !RESERVED_TOP_LEVEL_LABELS.contains(top_level)
}

/// Returns whether a string is a public unicast IPv4 or IPv6 address.
pub fn is_public_ip(value: &str) -> bool {
    value.parse().is_ok_and(is_global_unicast)
}

/// Returns whether a string is a public DNS hostname or unicast IP address.
pub fn is_public_host(value: &str) -> bool {
    is_public_hostname(value) || is_public_ip(value)
}

fn is_global_unicast(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_global_unicast_ipv4(address),
        IpAddr::V6(address) => is_global_unicast_ipv6(address),
    }
}

fn is_global_unicast_ipv4(address: Ipv4Addr) -> bool {
    let [a, b, c, d] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0 && d != 9 && d != 10)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn is_global_unicast_ipv6(address: Ipv6Addr) -> bool {
    let [a, b, ..] = address.segments();
    a & 0xe000 == 0x2000
        && !(a == 0x2001 && b < 0x0200)
        && !(a == 0x2001 && b == 0x0db8)
        && a != 0x2002
        && !(a == 0x3fff && b < 0x1000)
}

fn valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    (1..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

fn valid_top_level_label(label: &str) -> bool {
    if let Some(encoded) = label.strip_prefix("xn--") {
        return encoded.len() >= 2 && !encoded.ends_with('-');
    }
    (2..=63).contains(&label.len()) && label.bytes().all(|byte| byte.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_public_names_including_pontia_domains() {
        for name in [
            "edge.example.com",
            "a.b.c.example.co.uk",
            "xn--bcher-kva.example",
            "edge.xn--p1ai",
            "pontia.dev",
            "sg.edge.pontia.dev",
            "edge.pontia.dev",
            "pontia.run",
            "preview.pontia.run",
            "edge.example.test",
            "1.example.com",
        ] {
            assert!(is_public_hostname(name), "rejected {name}");
        }
    }

    #[test]
    fn rejects_ip_literals_local_names_and_non_canonical_forms() {
        let long_label = "a".repeat(64);
        let long_name = format!("{}.example", "a.".repeat(124));
        for name in [
            "",
            "example",
            "localhost",
            "edge.localhost",
            "printer.local",
            "service.internal",
            "1.0.0.127.in-addr.arpa",
            "router.home.arpa",
            "host.localdomain",
            "site.invalid",
            "hidden.onion",
            "name.alt",
            "127.0.0.1",
            "1.2.3.4",
            "0x7f.1",
            "::1",
            "[::1]",
            "example.123",
            "Edge.Example.com",
            "edge.example.com.",
            " edge.example.com",
            "-edge.example.com",
            "edge-.example.com",
            "edge..example.com",
            "edge_1.example.com",
            "edge.example.c",
            "edge.xn--",
            "edge.xn--abc-",
            "edge.xn--a",
            &format!("{long_label}.example"),
            &long_name,
        ] {
            assert!(!is_public_hostname(name), "accepted {name}");
        }
    }

    #[test]
    fn accepts_only_public_unicast_ip_hosts() {
        for address in ["8.8.8.8", "192.0.0.9", "2606:4700:4700::1111"] {
            assert!(is_public_ip(address), "rejected {address}");
            assert!(is_public_host(address), "rejected {address}");
        }
        for address in [
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "192.0.2.1",
            "::1",
            "2001:db8::1",
            "fc00::1",
            "[2606:4700:4700::1111]",
        ] {
            assert!(!is_public_ip(address), "accepted {address}");
            assert!(!is_public_host(address), "accepted {address}");
        }
    }

    #[test]
    fn port_policy_excludes_unsafe_ports_without_a_whitelist() {
        for port in UNSAFE_PORTS {
            assert!(!is_safe_port(*port));
        }
        assert!(!is_safe_port(0));
        for port in [80, 443, 444, 8443, 65535] {
            assert!(is_safe_port(port));
        }
    }
}
