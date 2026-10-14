//! Minimal SCGI transport for talking to an rtorrent daemon.
//!
//! SCGI requests are a single netstring wrapping a null-separated header block
//! (`CONTENT_LENGTH` and `SCGI: 1` are required) followed by the raw body.
//! rtorrent closes the connection after each response, so every call is a
//! fresh connection: connect, write, read to end.

use std::io::{Read, Write};
use std::path::PathBuf;

/// Where the rtorrent SCGI socket lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScgiTarget {
    /// Unix socket path (the recommended setup inside a container).
    UnixSocket(PathBuf),
    /// TCP address (`host`, `port`) — useful for local testing.
    Tcp(String, u16),
}

/// Encode one SCGI request (netstring + headers + body).
pub fn encode_request(extra_headers: &[(String, String)], body: &[u8]) -> Vec<u8> {
    let mut header_block: Vec<u8> = Vec::new();
    let mut push_header = |key: &str, value: &str| {
        header_block.extend_from_slice(key.as_bytes());
        header_block.push(0);
        header_block.extend_from_slice(value.as_bytes());
        header_block.push(0);
    };
    push_header("CONTENT_LENGTH", &body.len().to_string());
    push_header("SCGI", "1");
    for (key, value) in extra_headers {
        push_header(key, value);
    }

    let mut out = format!("{}:", header_block.len()).into_bytes();
    out.extend_from_slice(&header_block);
    out.push(b',');
    out.extend_from_slice(body);
    out
}

/// Parsed SCGI request: header pairs plus the remaining body bytes.
pub struct DecodedRequest<'a> {
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
}

/// Parse a netstring header block back into header pairs (used by tests and
/// by the container-side test harness).
pub fn decode_request_headers(raw: &[u8]) -> Result<DecodedRequest<'_>, String> {
    let colon = raw
        .iter()
        .position(|b| *b == b':')
        .ok_or_else(|| "scgi: missing netstring colon".to_string())?;
    let len: usize = std::str::from_utf8(&raw[..colon])
        .map_err(|e| format!("scgi: bad netstring length: {e}"))?
        .parse()
        .map_err(|e| format!("scgi: bad netstring length: {e}"))?;
    let after_colon = &raw[colon + 1..];
    if after_colon.len() < len + 1 || after_colon[len] != b',' {
        return Err("scgi: truncated netstring".to_string());
    }
    let header_block = &after_colon[..len];
    let body = &after_colon[len + 1..];

    // SCGI header blocks are a flat sequence: key\0value\0key\0value\0.
    let mut parts: Vec<String> = Vec::new();
    for fragment in header_block.split(|b| *b == 0) {
        let text = std::str::from_utf8(fragment).map_err(|e| format!("scgi: bad header: {e}"))?;
        if !text.is_empty() {
            parts.push(text.to_string());
        }
    }
    if !parts.len().is_multiple_of(2) {
        return Err("scgi: odd number of header components".to_string());
    }
    let mut headers = Vec::new();
    for pair in parts.chunks(2) {
        headers.push((pair[0].clone(), pair[1].clone()));
    }
    Ok(DecodedRequest { headers, body })
}

fn connect(target: &ScgiTarget) -> Result<Box<dyn ReadWrite>, String> {
    match target {
        ScgiTarget::UnixSocket(path) => {
            #[cfg(unix)]
            {
                let socket = std::os::unix::net::UnixStream::connect(path)
                    .map_err(|e| format!("rtorrent: cannot connect to unix socket: {e}"))?;
                Ok(Box::new(socket))
            }
            #[cfg(not(unix))]
            {
                let _ = path;
                Err("rtorrent: unix domain sockets are not supported on this platform".to_string())
            }
        }
        ScgiTarget::Tcp(host, port) => {
            let stream = std::net::TcpStream::connect((host.as_str(), *port))
                .map_err(|e| format!("rtorrent: cannot connect to tcp target: {e}"))?;
            Ok(Box::new(stream))
        }
    }
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

/// Perform one SCGI call and return the raw response body.
pub fn call(target: &ScgiTarget, body: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream = connect(target)?;
    let request = encode_request(&[], body);
    stream
        .write_all(&request)
        .map_err(|e| format!("rtorrent: write failed: {e}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| format!("rtorrent: read failed: {e}"))?;
    // rtorrent 0.16+ prefixes SCGI responses with HTTP-style headers
    // ("Status: 200 OK\r\n...\r\n\r\n"); 0.9.x returns bare XML. Strip the
    // header block when present.
    if response.starts_with(b"Status:") || response.starts_with(b"HTTP/") {
        for separator in [b"\r\n\r\n".as_slice(), b"\n\n".as_slice()] {
            if let Some(pos) = find_subslice(&response, separator) {
                return Ok(response[pos + separator.len()..].to_vec());
            }
        }
    }
    Ok(response)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_request_shape() {
        let raw = encode_request(&[("REQUEST_METHOD".into(), "POST".into())], b"<x/>");
        let text = String::from_utf8(raw.clone()).unwrap();
        // netstring opens with the header-block length (44 bytes) and a colon
        assert!(
            text.starts_with(
                "44:CONTENT_LENGTH\x004\x00SCGI\x001\x00REQUEST_METHOD\x00POST\x00,<x/>"
            ),
            "unexpected request encoding: {text}"
        );
        // and it round-trips through the decoder
        let decoded = decode_request_headers(&raw).unwrap();
        assert_eq!(decoded.body, b"<x/>");
        assert!(decoded
            .headers
            .contains(&("CONTENT_LENGTH".to_string(), "4".to_string())));
        assert!(decoded
            .headers
            .contains(&("SCGI".to_string(), "1".to_string())));
    }
}
