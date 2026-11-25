use nebulae_torrent_client_lib::engine::transmission::{RpcPolicy, TransmissionRpcClient};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn test_baseline_fast_mock_response() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "{\"result\":\"success\",\"arguments\":{\"torrents\":[]}}";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    let client = TransmissionRpcClient::new(port, "", "");
    let mut durations = Vec::new();

    for _ in 0..30 {
        let start = Instant::now();
        let res = client.torrent_get(None);
        let elapsed = start.elapsed();
        assert!(res.is_ok(), "fast mock read must succeed: {:?}", res.err());
        durations.push(elapsed);
    }

    durations.sort();
    let median = durations[durations.len() / 2];
    let p95 = durations[(durations.len() as f64 * 0.95) as usize];
    println!(
        "TIMING_FAST_MOCK (30 reps): median={:?}, p95={:?}",
        median, p95
    );
}

#[test]
fn test_baseline_connection_refused_duration() {
    // Bind to an ephemeral port and drop listener immediately so port is closed
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    drop(listener);

    let client = TransmissionRpcClient::new(port, "", "");
    let start = Instant::now();
    let res = client.torrent_get(None);
    let elapsed = start.elapsed();

    assert!(res.is_err(), "connection to closed port must fail");
    // Under patched RpcPolicy::InteractiveLocal, max 1 retry after 100ms:
    // Total elapsed should be well under 500ms (typically ~105-120ms on macOS).
    println!(
        "TIMING_CONNECTION_REFUSED: elapsed={:?}, err={:?}",
        elapsed,
        res.err()
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "Refused connection must complete under 500ms, took {:?}",
        elapsed
    );
}

#[test]
fn test_baseline_hung_slow_response_duration() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_clone = attempts.clone();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            attempts_clone.fetch_add(1, Ordering::SeqCst);
            // Read incoming request so TCP handshake finishes, then sleep without writing
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            std::thread::sleep(Duration::from_secs(6));
        }
    });

    let client = TransmissionRpcClient::new(port, "", "");
    let start = Instant::now();
    // BackgroundLocal has a 3s total budget and 0 retries on timeout
    let res = client.torrent_get_with_policy(None, RpcPolicy::BackgroundLocal);
    let elapsed = start.elapsed();

    assert!(res.is_err(), "hung server must time out");
    println!(
        "TIMING_HUNG_SLOW_RESPONSE: elapsed={:?}, attempts={}",
        elapsed,
        attempts.load(Ordering::SeqCst)
    );
    // Verify response timeout is NOT retried (attempts == 1)
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "Response timeout must not trigger retries"
    );
    assert!(
        elapsed <= Duration::from_millis(3500),
        "Hung response must abort at policy budget (3s), took {:?}",
        elapsed
    );
}

#[test]
fn test_baseline_csrf_409_negotiation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    let req_count = Arc::new(AtomicUsize::new(0));
    let count_clone = req_count.clone();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let n = count_clone.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                // First request gets 409 Conflict with session id
                let resp = "HTTP/1.1 409 Conflict\r\nX-Transmission-Session-Id: mock-token-123\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(resp.as_bytes());
            } else {
                // Subsequent request gets 200 OK
                let body = "{\"result\":\"success\",\"arguments\":{\"torrents\":[]}}";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
            }
            let _ = stream.flush();
        }
    });

    let client = TransmissionRpcClient::new(port, "", "");
    let start = Instant::now();
    let res = client.torrent_get(None);
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "409 negotiation should succeed on retry");
    assert_eq!(
        req_count.load(Ordering::SeqCst),
        2,
        "Must perform exactly one renegotiation replay"
    );
    println!("TIMING_409_NEGOTIATION: elapsed={:?}", elapsed);
}

#[test]
fn test_baseline_malformed_json() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let body = "<html><body>502 Bad Gateway</body></html>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    let client = TransmissionRpcClient::new(port, "", "");
    let start = Instant::now();
    let res = client.torrent_get(None);
    let elapsed = start.elapsed();

    assert!(res.is_err(), "malformed JSON body must fail");
    println!(
        "TIMING_MALFORMED_JSON: elapsed={:?}, err={:?}",
        elapsed,
        res.err()
    );
}

#[test]
fn test_baseline_synthetic_fixtures() {
    for (torrent_count, peer_count) in [(100, 0), (1000, 50), (5000, 500)] {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("port").port();

        let peers_json: Vec<serde_json::Value> = (0..peer_count)
            .map(|p| {
                serde_json::json!({
                    "address": format!("192.168.1.{}", p % 250),
                    "port": 50000 + (p % 1000),
                    "clientName": "Transmission 4.0.5",
                    "rateToClient": 1024 * 1024,
                    "rateToPeer": 512 * 1024,
                    "progress": 0.5,
                    "flagStr": "DTE"
                })
            })
            .collect();

        let torrents_json: Vec<serde_json::Value> = (0..torrent_count)
            .map(|i| {
                serde_json::json!({
                    "id": i,
                    "hashString": format!("{:040x}", i),
                    "name": format!("Ubuntu Linux 24.04.{}.iso", i),
                    "status": 4,
                    "percentDone": 0.75,
                    "downloadedEver": 1500000000i64,
                    "uploadedEver": 500000000i64,
                    "secondsDownloading": 300,
                    "secondsSeeding": 0,
                    "rateDownload": 10000000,
                    "rateUpload": 500000,
                    "sizeWhenDone": 2000000000i64,
                    "totalSize": 2000000000i64,
                    "leftUntilDone": 500000000i64,
                    "downloadDir": "/Downloads",
                    "addedDate": 1700000000,
                    "doneDate": 0,
                    "eta": 50,
                    "errorString": "",
                    "peersConnected": peer_count,
                    "peers": if i == 0 { &peers_json } else { &peers_json[..0] }
                })
            })
            .collect();

        let body = serde_json::json!({
            "result": "success",
            "arguments": {
                "torrents": torrents_json
            }
        })
        .to_string();

        let body_bytes = body.into_bytes();
        let body_len = body_bytes.len();
        let body_bytes_arc = Arc::new(body_bytes);

        let body_clone = body_bytes_arc.clone();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body_len
                );
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(&body_clone);
                let _ = stream.flush();
            }
        });

        let client = TransmissionRpcClient::new(port, "", "");
        let start = Instant::now();
        let res = client.torrent_get(None);
        let elapsed = start.elapsed();

        assert!(res.is_ok(), "synthetic read failed: {:?}", res.err());
        let count = res.unwrap().len();
        println!(
            "TIMING_SYNTHETIC: torrents={}, peers={}, bytes={}, elapsed={:?}",
            count, peer_count, body_len, elapsed
        );
    }
}

#[test]
fn test_rpc_policies_matrix() {
    assert_eq!(
        RpcPolicy::InteractiveLocal.total_budget(),
        Duration::from_secs(5)
    );
    assert_eq!(
        RpcPolicy::InteractiveLocal.connect_timeout(),
        Duration::from_secs(1)
    );
    assert_eq!(RpcPolicy::InteractiveLocal.max_connect_retries(), 1);
    assert_eq!(
        RpcPolicy::InteractiveLocal.retry_delay(),
        Duration::from_millis(100)
    );

    assert_eq!(
        RpcPolicy::BackgroundLocal.total_budget(),
        Duration::from_secs(3)
    );
    assert_eq!(
        RpcPolicy::BackgroundLocal.connect_timeout(),
        Duration::from_secs(1)
    );
    assert_eq!(RpcPolicy::BackgroundLocal.max_connect_retries(), 0);
    assert_eq!(RpcPolicy::BackgroundLocal.retry_delay(), Duration::ZERO);

    assert_eq!(RpcPolicy::Mutation.total_budget(), Duration::from_secs(30));
    assert_eq!(
        RpcPolicy::Mutation.connect_timeout(),
        Duration::from_secs(5)
    );
    assert_eq!(RpcPolicy::Mutation.max_connect_retries(), 0);
    assert_eq!(RpcPolicy::Mutation.retry_delay(), Duration::ZERO);
}

#[test]
fn test_torrent_get_peers_narrow_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("port").port();
    let received_body = Arc::new(std::sync::Mutex::new(String::new()));
    let received_body_clone = received_body.clone();

    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]).to_string();
            if let Some(pos) = req_str.find("\r\n\r\n") {
                *received_body_clone.lock().unwrap() = req_str[pos + 4..].to_string();
            }

            let body = "{\"result\":\"success\",\"arguments\":{\"torrents\":[{\"id\":1,\"hashString\":\"abc\",\"peers\":[],\"peerStats\":[]}]}}";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    let client = TransmissionRpcClient::new(port, "", "");
    let res = client.torrent_get_peers("1");
    assert!(
        res.is_ok(),
        "torrent_get_peers must succeed: {:?}",
        res.err()
    );

    let body_str = received_body.lock().unwrap().clone();
    let body_json: serde_json::Value = serde_json::from_str(&body_str).expect("parse json");
    let fields = body_json["arguments"]["fields"]
        .as_array()
        .expect("fields array");

    let field_names: Vec<&str> = fields.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(field_names, vec!["id", "hashString", "peers", "peerStats"]);
    // Must NOT contain heavy files/fileStats arrays
    assert!(!field_names.contains(&"files"));
    assert!(!field_names.contains(&"fileStats"));
}
