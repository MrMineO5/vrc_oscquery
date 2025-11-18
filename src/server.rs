use crate::client::discover_vrchat_oscquery;
use crate::node::OscNode;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
struct SharedState {
    root: Arc<RwLock<OscNode>>,
    host_info: Arc<HostInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostInfo {
    #[serde(rename = "NAME")]
    pub name: String,
    #[serde(rename = "OSC_IP")]
    pub osc_ip: String,
    #[serde(rename = "OSC_PORT")]
    pub osc_port: u16,
    #[serde(rename = "OSC_TRANSPORT")]
    pub osc_transport: String,
    #[serde(rename = "EXTENSIONS")]
    pub extensions: serde_json::Value,
}

pub struct OscQueryServerBuilder {
    app_name: String,
    bind_ip: IpAddr,
    http_port: u16,
    osc_port: u16,
    root: OscNode,
}

#[derive(Debug, thiserror::Error)]
pub enum OscQueryServerError {
    #[error("IO error: {0}")]
    ListenError(#[from] std::io::Error),

    #[error("IO error: {0}")]
    MdnsError(#[from] mdns_sd::Error),
}

impl OscQueryServerBuilder {
    pub fn new(app_name: impl Into<String>, osc_port: u16) -> Self {
        Self {
            app_name: app_name.into(),
            bind_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            http_port: 0,
            osc_port,
            root: OscNode::new_container("/"),
        }
    }

    pub fn with_bind_ip(mut self, ip: IpAddr) -> Self {
        self.bind_ip = ip;
        self
    }

    pub fn with_http_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    /// Receive all VRChat avatar parameters
    ///
    /// This makes sure `/avatar` exists so VRChat will auto-route
    /// `/avatar/change` and `/avatar/parameters/*` to your OSC port.
    pub fn with_vrchat_avatar_receiver(mut self) -> Self {
        // Ensure /avatar container exists
        OscNode::ensure_path(&mut self.root, "/avatar");
        self
    }

    /// Receive VRChat tracking data
    pub fn with_vrchat_tracking_receiver(mut self) -> Self {
        OscNode::ensure_path(&mut self.root, "/tracking/vrsystem");
        self
    }

    pub async fn build_and_run(self) -> Result<RunningServer, OscQueryServerError> {
        // Bind HTTP
        let http_listener =
            tokio::net::TcpListener::bind(SocketAddr::new(self.bind_ip, self.http_port)).await?;
        let local_addr = http_listener.local_addr()?;
        let http_port = local_addr.port();

        println!(
            "OSCQuery HTTP server listening on {}:{}",
            self.bind_ip, http_port
        );

        let host_info = HostInfo {
            name: self.app_name.clone(),
            osc_ip: self.bind_ip.to_string(),
            osc_port: self.osc_port,
            osc_transport: "UDP".to_string(),
            extensions: serde_json::json!({}), // no extensions yet
        };

        let state = SharedState {
            root: Arc::new(RwLock::new(self.root)),
            host_info: Arc::new(host_info),
        };

        tokio::task::spawn(async move {
            loop {
                let shared = state.clone();

                let (stream, _) = http_listener.accept().await.unwrap();

                // Use an adapter to access something implementing `tokio::io` traits as if they implement
                // `hyper::rt` IO traits.
                let io = TokioIo::new(stream);

                // Spawn a tokio task to serve multiple connections concurrently
                tokio::task::spawn(async move {
                    // Finally, we bind the incoming connection to our `hello` service
                    if let Err(err) = http1::Builder::new()
                        // `service_fn` converts our function in a `Service`
                        .serve_connection(io, service_fn(|req| handle_request(req, shared.clone())))
                        .await
                    {
                        eprintln!("Error serving connection: {:?}", err);
                    }
                });
            }
        });

        let mdns = ServiceDaemon::new()?;


        let service_type_oscquery = "_oscjson._tcp.local.";

        let host_name = format!("{}.oscjson.local.", self.app_name);
        let addr_ipv4 = Ipv4Addr::LOCALHOST;

        let mut props_oscquery = HashMap::new();
        props_oscquery.insert("name".to_string(), self.app_name.clone());
        props_oscquery.insert("osc_port".to_string(), self.osc_port.to_string());
        props_oscquery.insert("osc_transport".to_string(), "UDP".to_string());

        let info_oscquery = ServiceInfo::new(
            service_type_oscquery,
            &self.app_name,
            &host_name,
            IpAddr::V4(addr_ipv4),
            http_port,
            props_oscquery,
        )?;

        mdns.register(info_oscquery)?;


        let service_type_osc = "_osc._udp.local.";

        let mut props_osc = HashMap::new();
        props_osc.insert("name".to_string(), self.app_name.clone());

        let info_osc = ServiceInfo::new(
            service_type_osc,
            &self.app_name,
            &host_name,
            IpAddr::V4(addr_ipv4),
            self.osc_port,
            props_osc,
        )?;

        mdns.register(info_osc)?;


        // For some reason we need to wait and then query the mDNS service for VRChat to find it...?
        sleep(Duration::from_secs(1)).await;

        discover_vrchat_oscquery(Duration::from_secs(5)).await.unwrap();

        Ok(RunningServer { _mdns: mdns })
    }
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    state: SharedState,
) -> Result<Response<String>, Infallible> {
    let uri = req.uri();
    let query = uri.query().unwrap_or("");

    if query.eq_ignore_ascii_case("HOST_INFO") {
        let json = serde_json::to_string(&*state.host_info).unwrap_or_else(|_| "".to_string());
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/json")
            .body(json)
            .unwrap());
    }

    let root = state.root.read().unwrap();
    let json = serde_json::to_string(&*root).unwrap_or_else(|_| "{}".to_string());

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(json)
        .unwrap())
}

pub struct RunningServer {
    pub _mdns: ServiceDaemon,
}
