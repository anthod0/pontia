const UNSAFE_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668, 6669,
    6697, 10080,
];

pub fn parse_edge_port(value: &str) -> Result<u16, String> {
    let port = value
        .parse::<u16>()
        .map_err(|_| "port must be an integer from 1 to 65535".to_owned())?;
    validate_edge_port(port)?;
    Ok(port)
}

pub fn validate_edge_port(port: u16) -> Result<(), String> {
    if port == 0 || UNSAFE_PORTS.contains(&port) {
        return Err(
            "port is blocked by browsers or the control-plane runtime; try 8443".to_owned(),
        );
    }
    Ok(())
}

pub fn tunnel_url(hostname: &str, port: u16) -> String {
    if port == 443 {
        format!("wss://{hostname}/tunnel")
    } else {
        format!("wss://{hostname}:{port}/tunnel")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_policy_excludes_unsafe_ports_without_a_whitelist() {
        for port in UNSAFE_PORTS {
            assert!(validate_edge_port(*port).is_err());
        }
        for port in [80, 443, 444, 8443, 65535] {
            assert!(validate_edge_port(port).is_ok());
        }
        for value in ["0", "65536", "-1", "dns", "25", "6000"] {
            assert!(parse_edge_port(value).is_err());
        }
    }
}
