use std::{
    io::{self, ErrorKind},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};
use tokio::{net::TcpStream, time::timeout};

// https://fetch.spec.whatwg.org/#port-blocking
const UNSAFE_PORTS: &[u16] = &[
    0, 1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101,
    102, 103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427,
    465, 512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990,
    993, 995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667,
    6668, 6669, 6679, 6697, 10080,
];

pub(super) fn forbidden(port: u16, bound_port: u16) -> bool {
    port == bound_port || UNSAFE_PORTS.contains(&port)
}

async fn dial(addr: SocketAddr) -> io::Result<TcpStream> {
    timeout(Duration::from_secs(2), TcpStream::connect(addr)).await?
}

pub(super) async fn connect(port: u16) -> io::Result<TcpStream> {
    let stream = match dial((Ipv4Addr::LOCALHOST, port).into()).await {
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::ConnectionRefused
                    | ErrorKind::AddrNotAvailable
                    | ErrorKind::NetworkUnreachable
            ) =>
        {
            dial((Ipv6Addr::LOCALHOST, port).into()).await?
        }
        result => result?,
    };
    if !stream.peer_addr()?.ip().is_loopback() {
        return Err(ErrorKind::PermissionDenied.into());
    }
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permits_development_ports_and_denies_zero() {
        assert!(forbidden(0, 5432));
        for port in [80, 443, 3000, 4173, 5173, 8000, 8080, 65535] {
            assert!(!forbidden(port, 5432));
        }
    }
}
