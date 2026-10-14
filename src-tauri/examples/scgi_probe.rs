//! Diagnostic: one raw SCGI round trip against a live rtorrent socket.
//! Prints the raw response bytes and the parsed XML-RPC value — useful when
//! wiring up a new rtorrent version or debugging a socket path.
//! Usage: cargo run --example scgi_probe -- /path/to/scgi.sock

use nebulae_torrent_client_lib::engine::rtorrent::{rpc, scgi};

fn main() {
    let path = std::env::args().nth(1).expect("socket path required");
    let target = scgi::ScgiTarget::UnixSocket(path.into());

    let body = rpc::build_call("system.client_version", &[]);
    let raw = scgi::call(&target, body.as_bytes()).expect("scgi call");
    println!("raw bytes ({}):", raw.len());
    println!(
        "{:?}",
        String::from_utf8_lossy(&raw)
            .chars()
            .take(300)
            .collect::<String>()
    );
    let parsed = rpc::parse_response(&String::from_utf8_lossy(&raw));
    println!("parsed: {parsed:?}");
}
