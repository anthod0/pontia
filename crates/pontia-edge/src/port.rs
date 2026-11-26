use pontia_tunnel::endpoint::is_safe_port;

pub fn parse_edge_port(value: &str) -> Result<u16, String> {
    let port = value
        .parse::<u16>()
        .map_err(|_| "port must be an integer from 1 to 65535".to_owned())?;
    validate_edge_port(port)?;
    Ok(port)
}

pub fn validate_edge_port(port: u16) -> Result<(), String> {
    if !is_safe_port(port) {
        return Err(
            "port is blocked by browsers or the control-plane runtime; try 8443".to_owned(),
        );
    }
    Ok(())
}

pub fn tunnel_url(hostname: &str, port: u16) -> String {
    let host = if hostname.contains(':') {
        format!("[{hostname}]")
    } else {
        hostname.to_owned()
    };
    if port == 443 {
        format!("wss://{host}/tunnel")
    } else {
        format!("wss://{host}:{port}/tunnel")
    }
}

#[cfg(test)]
mod tests {
    use pontia_tunnel::endpoint::UNSAFE_PORTS;

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

    #[test]
    fn tunnel_urls_bracket_ipv6_addresses() {
        assert_eq!(tunnel_url("8.8.8.8", 443), "wss://8.8.8.8/tunnel");
        assert_eq!(
            tunnel_url("2606:4700:4700::1111", 443),
            "wss://[2606:4700:4700::1111]/tunnel"
        );
        assert_eq!(
            tunnel_url("2606:4700:4700::1111", 8443),
            "wss://[2606:4700:4700::1111]:8443/tunnel"
        );
    }
}
