//! Public edge endpoint rules shared by devices and edges.

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
