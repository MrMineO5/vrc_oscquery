use std::net::UdpSocket;
use std::thread::sleep;
use std::time::Duration;
use vrc_oscquery::client::discover_vrchat_oscquery;
use vrc_oscquery::server::{OscQueryServerBuilder, RunningServer};

#[tokio::main]
async fn main() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let udp_port = socket.local_addr().unwrap().port();

    println!("Starting OSC receiver on UDP port {}", udp_port);

    let result = OscQueryServerBuilder::new("TestApp", udp_port)
        .with_vrchat_avatar_receiver()
        .build_and_run().await.unwrap();

    loop {

    }
}