use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct DiscoveredOscQueryService {
    pub instance_name: String, // e.g. "VRChat-Client-123456._oscjson._tcp.local."
    pub host_name: String,
    pub addr_v4: Ipv4Addr,
    pub port: u16,
}

/// Errors, errors, errors
#[derive(Debug, Error)]
pub enum OscQueryError {
    #[error("mDNS error: {0}")]
    Mdns(#[from] mdns_sd::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Discovery timed out without finding a VRChat OSCQuery service")]
    DiscoveryTimeout,

    #[error("mDNS channel closed while waiting for VRChat OSCQuery service")]
    DiscoveryChannelClosed,
}

pub async fn discover_vrchat_oscquery(
    timeout: Duration,
) -> Result<DiscoveredOscQueryService, OscQueryError> {
    let mdns = ServiceDaemon::new()?;
    let receiver = mdns.browse("_oscjson._tcp.local.")?;

    let deadline = Instant::now() + timeout;

    loop {
        let remaining = match deadline.checked_duration_since(Instant::now()) {
            Some(d) if !d.is_zero() => d,
            _ => {
                mdns.shutdown().ok();
                return Err(OscQueryError::DiscoveryTimeout);
            }
        };

        let event_res =
            tokio::time::timeout(remaining, receiver.recv_async()).await;

        let event = match event_res {
            Ok(Ok(ev)) => ev,
            Ok(Err(_)) => {
                mdns.shutdown().ok();
                return Err(OscQueryError::DiscoveryChannelClosed);
            }
            Err(_) => {
                mdns.shutdown().ok();
                return Err(OscQueryError::DiscoveryTimeout);
            }
        };

        match event {
            ServiceEvent::ServiceResolved(info) => {
                if info.ty_domain == "_oscjson._tcp.local."
                    && info.fullname.starts_with("VRChat-Client-")
                {
                    let v4_addrs = info.get_addresses_v4();
                    let addr = v4_addrs
                        .iter()
                        .next()
                        .cloned()
                        .unwrap_or(Ipv4Addr::LOCALHOST);

                    let out = DiscoveredOscQueryService {
                        instance_name: info.fullname.clone(),
                        host_name: info.host.clone(),
                        addr_v4: addr,
                        port: info.port,
                    };

                    mdns.shutdown().ok();
                    return Ok(out);
                }
            }
            _ => {
                // Ignore other events.
            }
        }
    }
}
