//! The addresses players use to connect: this computer on the local network, and the public IP of the
//! network (looked up from api.ipify.org every 30 minutes; it only answers with your IP).

use std::net::IpAddr;
use std::time::Duration;

use crate::state::Shared;

/// This computer's address on the local network (no packet is sent).
pub fn lan_ip() -> Option<IpAddr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

/// A plausible public IPv4/IPv6 address (rejects HTML error pages and private ranges).
pub fn parse_public_ip(text: &str) -> Option<String> {
    let ip: IpAddr = text.trim().parse().ok()?;
    let private = match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    };
    (!private).then(|| ip.to_string())
}

pub fn spawn_public_ip(state: Shared) {
    let st = state.clone();
    state.rt.spawn(async move {
        loop {
            if st.is_quitting() {
                break;
            }
            let found = async {
                let text = st.http.get("https://api.ipify.org").timeout(Duration::from_secs(10)).send().await.ok()?.text().await.ok()?;
                parse_public_ip(&text)
            }
            .await;
            if found.is_some() && *st.public_ip.read() != found {
                *st.public_ip.write() = found;
                st.notify();
            }
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ip_is_validated() {
        assert_eq!(parse_public_ip("203.0.113.7\n").as_deref(), Some("203.0.113.7"));
        assert_eq!(parse_public_ip("192.168.1.2"), None);
        assert_eq!(parse_public_ip("<html>error</html>"), None);
        assert_eq!(parse_public_ip("2001:db8::1").as_deref(), Some("2001:db8::1"));
    }
}
