use std::sync::LazyLock;

static UNSAFE_PORTS: LazyLock<Vec<u16>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../config/edge-unsafe-ports.json"))
        .expect("shared unsafe-port policy must be valid")
});

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
    fn shared_port_policy_excludes_unsafe_ports_without_a_whitelist() {
        for port in UNSAFE_PORTS.iter() {
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
