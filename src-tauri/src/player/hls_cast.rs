//! HLS packaging + hardened LAN HTTP server for disc casting.
//!
//! WHY NOT PLAIN livehttp: measured on the Glass Fleet D1 ISO (broken VOB
//! timestamps) with a clean exit (SIGINT, 2026-11-28):
//! - without `mux=ts{use-key-frames}` the first segment never closes
//!   (EXTINF 54.09 s, 0-byte file; same with keyint alone); with it segments
//!   close on every disc and file measured;
//! - even then VLC's own index carries bad durations on the disc (a negative
//!   `#EXTINF:-1.56` and a 165.72 s final entry) because its durations come
//!   from the disc clock, not from the encoded video.
//!
//! So VLC only WRITES keyframe-aligned segments; the playlist is generated
//! here from the video PTS actually present in each segment, and the LAN
//! server serves that playlist plus the segment files.
//!
//! The server is deliberately tiny: GET/HEAD only, two path shapes
//! (`/index.m3u8` and `/seg-<8 digits>.ts`), peers must be private/loopback
//! addresses, bound to one interface (not 0.0.0.0), stopped by `stop()`
//! (called when the player session ends or the app exits).
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const TARGET_SEGMENT_S: u32 = 4;
pub const PLAYLIST_NAME: &str = "index.m3u8";
const MAX_CONNECTIONS: usize = 16;
const PTS_WRAP: u64 = 1 << 33;
const TS_PACKET: usize = 188;
/// One 29.97 fps frame in 90 kHz ticks; added to (last-first) PTS of the
/// final segment, whose successor does not exist to measure against.
const FRAME_TICKS: u64 = 3003;

/// sout chain: transcode to H.264/AAC with keyframes every ~2 s, then write
/// keyframe-aligned MPEG-TS segments into `dir`. Pure; unit-tested.
pub fn build_sout_chain(dir: &Path) -> Result<String, String> {
    let d = dir.to_string_lossy().replace('\\', "/");
    if d.contains('"') {
        return Err("segment directory contains a quote".into());
    }
    Ok(format!(
        ":sout=#transcode{{vcodec=h264,vb=2500,venc=x264{{profile=high,level=4.0,preset=veryfast,keyint=50,min-keyint=25}},\
acodec=mp4a,ab=192,channels=2,samplerate=44100}}:std{{access=livehttp{{seglen={TARGET_SEGMENT_S},delsegs=false,numsegs=0,\
index=\"{d}/vlc-index.m3u8\",index-url=seg-########.ts}},mux=ts{{use-key-frames}},dst=\"{d}/seg-########.ts\"}}"
    ))
}

/// `seg-` + 8 digits + `.ts` and nothing else.
pub fn is_segment_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 15
        && name.starts_with("seg-")
        && name.ends_with(".ts")
        && b[4..12].iter().all(u8::is_ascii_digit)
}

/// Private, loopback, link-local or CGNAT (Tailscale) peers only.
pub fn is_lan_peer(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            v.is_loopback()
                || v.is_private()
                || v.is_link_local()
                || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v) => {
            let s = v.segments();
            v.is_loopback() || (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfe80
        }
    }
}

fn read_pts(p: &[u8]) -> u64 {
    (((p[0] as u64 >> 1) & 7) << 30)
        | ((p[1] as u64) << 22)
        | (((p[2] as u64) >> 1) << 15)
        | ((p[3] as u64) << 7)
        | ((p[4] as u64) >> 1)
}

/// (first, max, frames): first and max video PES PTS (90 kHz ticks) and the
/// number of video PES found in `ts`, scanning 188-byte packets (video
/// stream ids 0xE0..=0xEF; the transcode emits only H.264 video there).
/// `None` when no video PES with a PTS is present.
pub fn scan_pts(ts: &[u8]) -> Option<(u64, u64, u32)> {
    let mut first = None;
    let mut max: u64 = 0;
    let mut frames: u32 = 0;
    let mut off = 0;
    while off + TS_PACKET <= ts.len() {
        let p = &ts[off..off + TS_PACKET];
        off += TS_PACKET;
        if p[0] != 0x47 || p[1] & 0x40 == 0 {
            continue;
        }
        let afc = (p[3] >> 4) & 3;
        if afc & 1 == 0 {
            continue;
        }
        let mut i = 4;
        if afc == 3 {
            i += 1 + p[4] as usize;
        }
        if i + 14 > TS_PACKET || p[i] != 0 || p[i + 1] != 0 || p[i + 2] != 1 {
            continue;
        }
        if !(0xE0..=0xEF).contains(&p[i + 3]) || p[i + 7] & 0x80 == 0 {
            continue;
        }
        let pts = read_pts(&p[i + 9..i + 14]);
        frames += 1;
        match first {
            None => {
                first = Some(pts);
                max = pts;
            }
            Some(f) => {
                if pts.wrapping_sub(f) % PTS_WRAP > max.wrapping_sub(f) % PTS_WRAP {
                    max = pts;
                }
            }
        }
    }
    first.map(|f| (f, max, frames))
}

/// One finished segment as seen by the playlist builder.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub name: String,
    pub first_pts: u64,
    /// first-to-last PTS + one frame; only used for the final segment.
    pub own_ticks: u64,
    /// Video frames (PES) in the segment.
    pub frames: u32,
}

/// Playlist from measured data. Normally a segment lasts until the next
/// segment's first PTS. MEASURED on the Glass Fleet D1 ISO: some stretches
/// carry thousands of frames whose PTS are squashed into a few seconds (the
/// disc's broken timestamps survive the transcode), so the PTS gap is
/// checked against the frame count (at the stream's own ticks-per-frame,
/// the median of the sane segments): a gap that disagrees by more than 2x
/// is replaced by frames x ticks-per-frame and followed by
/// #EXT-X-DISCONTINUITY. A zero/backwards/over-12 s gap is a break too.
/// `ended` adds the final segment and ENDLIST; otherwise the newest segment
/// (possibly still being written) is withheld. Pure; unit-tested.
pub fn build_playlist(segs: &[Seg], ended: bool) -> String {
    let listed = if ended {
        segs.len()
    } else {
        segs.len().saturating_sub(1)
    };
    let max_ticks = 3 * TARGET_SEGMENT_S as u64 * 90_000;
    let gap = |i: usize| -> Option<u64> {
        let n = segs.get(i + 1)?;
        Some(n.first_pts.wrapping_sub(segs[i].first_pts) % PTS_WRAP)
    };
    let mut ratios: Vec<u64> = (0..segs.len())
        .filter_map(|i| {
            let g = gap(i)?;
            let f = segs[i].frames as u64;
            (f > 0 && g > 0 && g <= max_ticks && (1500..=7000).contains(&(g / f))).then(|| g / f)
        })
        .collect();
    ratios.sort_unstable();
    let tpf = ratios.get(ratios.len() / 2).copied().unwrap_or(FRAME_TICKS);
    let mut entries: Vec<(f64, bool, &str)> = Vec::new();
    let mut disc_next = false;
    for (i, s) in segs.iter().enumerate().take(listed) {
        let expected = s.frames as u64 * tpf;
        let agrees =
            |d: u64| d > 0 && d <= max_ticks && d * 2 >= expected && d <= expected * 2 + 4 * 90_000;
        let (dur_ticks, brk) = match gap(i) {
            Some(d) if agrees(d) => (d, false),
            Some(_) if expected > 0 => (expected, true),
            Some(_) => (TARGET_SEGMENT_S as u64 * 90_000, true),
            None if expected > 0
                && !(s.own_ticks * 2 >= expected && s.own_ticks <= expected * 2 + 4 * 90_000) =>
            {
                (expected, false)
            }
            None => (s.own_ticks.clamp(FRAME_TICKS, max_ticks), false),
        };
        entries.push((dur_ticks as f64 / 90_000.0, disc_next, &s.name));
        disc_next = brk;
    }
    let target = entries
        .iter()
        .map(|e| e.0.ceil() as u32)
        .max()
        .unwrap_or(TARGET_SEGMENT_S)
        .max(1);
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:EVENT\n"
    );
    for (d, disc, name) in entries {
        if disc {
            out.push_str("#EXT-X-DISCONTINUITY\n");
        }
        out.push_str(&format!("#EXTINF:{d:.3},\n{name}\n"));
    }
    if ended {
        out.push_str("#EXT-X-ENDLIST\n");
    }
    out
}

struct Shared {
    root: PathBuf,
    ended: Box<dyn Fn() -> bool + Send + Sync>,
    /// segment name -> (first_pts, own_ticks, frames) for finished segments.
    cache: Mutex<HashMap<String, (u64, u64, u32)>>,
    stop: AtomicBool,
    active: AtomicUsize,
}

impl Shared {
    fn segments(&self, ended: bool) -> Vec<Seg> {
        let mut names: Vec<String> = std::fs::read_dir(&self.root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| is_segment_name(n))
            .collect();
        names.sort();
        let count = names.len();
        let mut out = Vec::new();
        for (i, n) in names.into_iter().enumerate() {
            let last = i + 1 == count;
            let known = self
                .cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&n)
                .copied();
            let (first, own, frames) = match known {
                Some(v) => v,
                None => {
                    let Some(bytes) = read_segment(&self.root.join(&n)) else {
                        break;
                    };
                    let Some((f, m, frames)) = scan_pts(&bytes) else {
                        break;
                    };
                    let own = m.wrapping_sub(f) % PTS_WRAP + FRAME_TICKS;
                    // The newest segment may still be growing; cache only
                    // finished ones.
                    if !last || ended {
                        self.cache
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(n.clone(), (f, own, frames));
                    }
                    (f, own, frames)
                }
            };
            out.push(Seg {
                name: n,
                first_pts: first,
                own_ticks: own,
                frames,
            });
        }
        out
    }

    fn playlist(&self) -> String {
        let ended = (self.ended)();
        build_playlist(&self.segments(ended), ended)
    }
}

fn read_segment(path: &Path) -> Option<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 {
        return None;
    }
    std::fs::read(path).ok()
}
pub struct LanServer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    pub addr: SocketAddr,
}

impl LanServer {
    /// Bind `bind:port` (falling back to an ephemeral port if `port` is busy)
    /// and serve `root`. `ended` reports whether the encoder has finished.
    pub fn start(
        root: PathBuf,
        bind: IpAddr,
        port: u16,
        ended: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::new(bind, port))
            .or_else(|_| TcpListener::bind(SocketAddr::new(bind, 0)))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            root,
            ended,
            cache: Mutex::new(HashMap::new()),
            stop: AtomicBool::new(false),
            active: AtomicUsize::new(0),
        });
        let sh = shared.clone();
        let thread = std::thread::Builder::new()
            .name("hls-lan-server".into())
            .spawn(move || accept_loop(listener, sh))?;
        Ok(Self {
            shared,
            thread: Some(thread),
            addr,
        })
    }

    /// Segments a player could fetch right now (the newest one is withheld
    /// until the encoder moves on, as in the playlist).
    pub fn ready_segments(&self) -> usize {
        let ended = (self.shared.ended)();
        let n = self.shared.segments(ended).len();
        if ended {
            n
        } else {
            n.saturating_sub(1)
        }
    }

    pub fn playlist_url(&self, host: &str) -> String {
        format!("http://{host}:{}/{PLAYLIST_NAME}", self.addr.port())
    }

    /// Stop accepting, join the accept thread, wait (bounded) for in-flight
    /// requests so the `ended` callback is never called after this returns.
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.shared.active.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for LanServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(listener: TcpListener, sh: Arc<Shared>) {
    while !sh.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                if !is_lan_peer(peer.ip()) {
                    continue;
                }
                if sh.active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                    let _ = (&stream).write_all(
                        b"HTTP/1.1 503 Busy\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                }
                sh.active.fetch_add(1, Ordering::SeqCst);
                let s2 = sh.clone();
                let spawned = std::thread::Builder::new()
                    .name("hls-lan-conn".into())
                    .spawn(move || {
                        let _ = handle(stream, &s2);
                        s2.active.fetch_sub(1, Ordering::SeqCst);
                    });
                if spawned.is_err() {
                    sh.active.fetch_sub(1, Ordering::SeqCst);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn respond(
    s: &mut TcpStream,
    status: &str,
    ctype: &str,
    extra: &str,
    head: bool,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n{extra}\r\n",
        body.len()
    )?;
    if !head {
        s.write_all(body)?;
    }
    s.flush()
}

fn parse_range(h: &str, len: u64) -> Option<(u64, u64)> {
    let spec = h.trim().strip_prefix("bytes=")?;
    let (a, b) = spec.split_once('-')?;
    if len == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len - 1)
    } else {
        let s: u64 = a.parse().ok()?;
        let e: u64 = if b.is_empty() {
            len - 1
        } else {
            b.parse().ok()?
        };
        (s, e.min(len - 1))
    };
    (start <= end && start < len).then_some((start, end))
}

fn handle(mut s: TcpStream, sh: &Shared) -> std::io::Result<()> {
    s.set_nonblocking(false)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        if buf.len() > 8192 {
            return respond(&mut s, "431 Too Large", "text/plain", "", true, b"");
        }
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut lines = text.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split(' ');
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let head = method == "HEAD";
    if method != "GET" && !head {
        return respond(
            &mut s,
            "405 Method Not Allowed",
            "text/plain",
            "Allow: GET, HEAD\r\n",
            true,
            b"",
        );
    }
    let path = target.split(['?', '#']).next().unwrap_or("");
    let name = path.strip_prefix('/').unwrap_or("");
    if name == PLAYLIST_NAME {
        let body = sh.playlist();
        return respond(
            &mut s,
            "200 OK",
            "application/vnd.apple.mpegurl",
            "Cache-Control: no-cache\r\n",
            head,
            body.as_bytes(),
        );
    }
    if !is_segment_name(name) {
        return respond(&mut s, "404 Not Found", "text/plain", "", head, b"");
    }
    let full = sh.root.join(name);
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(m) if m.is_file() => m,
        _ => return respond(&mut s, "404 Not Found", "text/plain", "", head, b""),
    };
    let len = meta.len();
    let range = lines
        .find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.eq_ignore_ascii_case("range").then(|| v.to_string())
        })
        .and_then(|v| parse_range(&v, len));
    let (start, end, status, extra) = match range {
        Some((a, b)) => (
            a,
            b,
            "206 Partial Content",
            format!("Content-Range: bytes {a}-{b}/{len}\r\n"),
        ),
        None => (0, len.saturating_sub(1), "200 OK", String::new()),
    };
    let n = if len == 0 { 0 } else { end - start + 1 };
    let mut f = std::fs::File::open(&full)?;
    f.seek(SeekFrom::Start(start))?;
    write!(
        s,
        "HTTP/1.1 {status}\r\nContent-Type: video/mp2t\r\nContent-Length: {n}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n{extra}\r\n"
    )?;
    if !head {
        let mut left = n;
        let mut chunk = vec![0u8; 64 * 1024];
        while left > 0 && !sh.stop.load(Ordering::SeqCst) {
            let want = chunk.len().min(left as usize);
            let got = f.read(&mut chunk[..want])?;
            if got == 0 {
                break;
            }
            s.write_all(&chunk[..got])?;
            left -= got as u64;
        }
    }
    s.flush()
}

/// Fallback bind address when no LAN interface is found.
pub fn loopback() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_pts(pts: u64) -> [u8; 5] {
        [
            0x21 | (((pts >> 30) & 7) << 1) as u8,
            (pts >> 22) as u8,
            0x01 | (((pts >> 15) & 0x7f) << 1) as u8,
            (pts >> 7) as u8,
            0x01 | ((pts & 0x7f) << 1) as u8,
        ]
    }

    /// One video PES (PUSI, no adaptation) per frame.
    fn make_ts(first_pts: u64, frames: u32, step: u64) -> Vec<u8> {
        let mut out = Vec::new();
        for k in 0..frames {
            let mut p = vec![0xFFu8; TS_PACKET];
            p[0] = 0x47;
            p[1] = 0x41;
            p[2] = 0x00;
            p[3] = 0x10;
            let pes = [0, 0, 1, 0xE0, 0, 0, 0x80, 0x80, 5];
            p[4..13].copy_from_slice(&pes);
            p[13..18].copy_from_slice(&enc_pts(first_pts + k as u64 * step));
            out.extend_from_slice(&p);
        }
        out
    }

    fn get(addr: SocketAddr, req: &str) -> (String, Vec<u8>) {
        let mut c = TcpStream::connect(addr).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        c.write_all(req.as_bytes()).unwrap();
        let mut all = Vec::new();
        let _ = c.read_to_end(&mut all);
        let split = all
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or(all.len());
        let head = String::from_utf8_lossy(&all[..split]).into_owned();
        let body = all.get(split + 4..).unwrap_or(&[]).to_vec();
        (head, body)
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("nebulae-hls-test-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sout_chain_forces_keyframes_and_quotes_paths() {
        let c = build_sout_chain(Path::new("/tmp/a b/x")).unwrap();
        assert!(c.contains("keyint=50,min-keyint=25"), "{c}");
        assert!(c.contains("mux=ts{use-key-frames}"), "{c}");
        assert!(
            c.contains("access=livehttp{seglen=4,delsegs=false,numsegs=0"),
            "{c}"
        );
        assert!(c.contains("dst=\"/tmp/a b/x/seg-########.ts\""), "{c}");
        assert!(build_sout_chain(Path::new("/tmp/q\"x")).is_err());
        let w = build_sout_chain(Path::new(r"C:\Temp\n")).unwrap();
        assert!(w.contains("C:/Temp/n/seg-########.ts"), "{w}");
    }

    #[test]
    fn segment_names_are_strict() {
        assert!(is_segment_name("seg-00000001.ts"));
        for bad in [
            "",
            "seg-1.ts",
            "seg-0000000a.ts",
            "../seg-00000001.ts",
            "seg-00000001.ts/",
            "seg-00000001.tsx",
            "index.m3u8",
            "seg-00000001.ts\0",
        ] {
            assert!(!is_segment_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn peers_must_be_lan() {
        for ok in [
            "127.0.0.1",
            "192.168.1.5",
            "10.0.0.2",
            "172.16.4.4",
            "169.254.1.1",
            "100.64.0.9",
            "::1",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(is_lan_peer(ok.parse().unwrap()), "{ok}");
        }
        for bad in ["8.8.8.8", "100.128.0.1", "172.32.0.1", "2001:db8::1"] {
            assert!(!is_lan_peer(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn scan_reads_first_and_max_pts() {
        let ts = make_ts(900_000, 5, 3003);
        assert_eq!(scan_pts(&ts), Some((900_000, 900_000 + 4 * 3003, 5)));
        assert_eq!(scan_pts(&[0u8; 400]), None);
        assert_eq!(
            scan_pts(&make_ts((1 << 33) - 3003, 3, 3003)).map(|x| x.1),
            Some(3003)
        );
    }

    #[test]
    fn playlist_uses_pts_gaps_and_flags_breaks() {
        let s = |n: &str, p: u64| Seg {
            name: n.into(),
            first_pts: p,
            own_ticks: 4 * 90_000,
            frames: 0,
        };
        let segs = vec![
            s("seg-00000000.ts", 0),
            s("seg-00000001.ts", 3 * 90_000),
            s("seg-00000002.ts", 90_000),
            s("seg-00000003.ts", 90_000 + 2 * 90_000),
        ];
        let live = build_playlist(&segs, false);
        assert!(live.contains("#EXTINF:3.000,\nseg-00000000.ts"), "{live}");
        assert!(live.contains("#EXTINF:4.000,\nseg-00000001.ts"), "{live}");
        assert!(
            live.contains("#EXT-X-DISCONTINUITY\n#EXTINF:2.000,\nseg-00000002.ts"),
            "{live}"
        );
        assert!(!live.contains("seg-00000003.ts"), "newest is withheld");
        assert!(!live.contains("ENDLIST"));
        assert!(!live.contains("EXTINF:-"), "never a negative duration");
        let done = build_playlist(&segs, true);
        assert!(
            done.contains("seg-00000003.ts") && done.trim_end().ends_with("#EXT-X-ENDLIST"),
            "{done}"
        );
        assert!(done.contains("#EXT-X-TARGETDURATION:4"));
    }

    #[test]
    fn squashed_timestamps_fall_back_to_the_frame_count() {
        // Measured shape (Glass Fleet D1): thousands of frames whose PTS span
        // only ~3 s. 120 frames per normal 4.004 s segment.
        let mk = |n: &str, p: u64, frames: u32| Seg {
            name: n.into(),
            first_pts: p,
            own_ticks: 0,
            frames,
        };
        let segs = vec![
            mk("seg-00000000.ts", 0, 120),
            mk("seg-00000001.ts", 360_360, 2700),
            mk("seg-00000002.ts", 360_360 + 270_000, 120),
            mk("seg-00000003.ts", 360_360 + 270_000 + 360_360, 120),
        ];
        let pl = build_playlist(&segs, true);
        assert!(pl.contains("#EXTINF:4.004,\nseg-00000000.ts"), "{pl}");
        assert!(pl.contains("#EXTINF:90.090,\nseg-00000001.ts"), "{pl}");
        assert!(
            pl.contains("#EXT-X-DISCONTINUITY\n#EXTINF:4.004,\nseg-00000002.ts"),
            "{pl}"
        );
        assert!(pl.contains("#EXT-X-TARGETDURATION:91"), "{pl}");
    }

    #[test]
    fn server_serves_playlist_and_segments_and_rejects_the_rest() {
        let dir = tmpdir("serve");
        let ended = Arc::new(AtomicBool::new(false));
        for (i, start) in [0u64, 360_000, 720_000].iter().enumerate() {
            std::fs::write(dir.join(format!("seg-{i:08}.ts")), make_ts(*start, 4, 3003)).unwrap();
        }
        std::fs::write(dir.join("secret.txt"), "nope").unwrap();
        let e2 = ended.clone();
        let mut srv = LanServer::start(
            dir.clone(),
            loopback(),
            0,
            Box::new(move || e2.load(Ordering::SeqCst)),
        )
        .unwrap();
        assert_eq!(
            srv.addr.ip(),
            loopback(),
            "bound to the requested interface"
        );
        let a = srv.addr;

        let (h, body) = get(a, "GET /index.m3u8 HTTP/1.1\r\nHost: x\r\n\r\n");
        let pl = String::from_utf8(body).unwrap();
        assert!(
            h.starts_with("HTTP/1.1 200") && h.contains("application/vnd.apple.mpegurl"),
            "{h}"
        );
        assert!(
            pl.contains("#EXTINF:4.000,\nseg-00000000.ts") && pl.contains("seg-00000001.ts"),
            "{pl}"
        );
        assert!(
            !pl.contains("seg-00000002.ts") && !pl.contains("ENDLIST"),
            "{pl}"
        );

        let (h, body) = get(a, "GET /seg-00000001.ts HTTP/1.1\r\n\r\n");
        assert!(
            h.starts_with("HTTP/1.1 200") && h.contains("video/mp2t"),
            "{h}"
        );
        assert_eq!(body, std::fs::read(dir.join("seg-00000001.ts")).unwrap());

        let (h, body) = get(
            a,
            "GET /seg-00000001.ts HTTP/1.1\r\nRange: bytes=188-375\r\n\r\n",
        );
        assert!(
            h.starts_with("HTTP/1.1 206") && h.contains("Content-Range: bytes 188-375/752"),
            "{h}"
        );
        assert_eq!(body.len(), 188);

        let (h, body) = get(a, "HEAD /seg-00000000.ts HTTP/1.1\r\n\r\n");
        assert!(h.starts_with("HTTP/1.1 200") && body.is_empty());

        for req in [
            "GET /../secret.txt HTTP/1.1\r\n\r\n",
            "GET /secret.txt HTTP/1.1\r\n\r\n",
            "GET /%2e%2e/secret.txt HTTP/1.1\r\n\r\n",
            "GET /seg-00000000.ts/../secret.txt HTTP/1.1\r\n\r\n",
            "GET //etc/passwd HTTP/1.1\r\n\r\n",
            "GET /seg-00000009.ts HTTP/1.1\r\n\r\n",
        ] {
            let (h, _) = get(a, req);
            assert!(h.starts_with("HTTP/1.1 404"), "{req:?} -> {h}");
        }
        let (h, _) = get(a, "POST /index.m3u8 HTTP/1.1\r\n\r\n");
        assert!(h.starts_with("HTTP/1.1 405"), "{h}");

        ended.store(true, Ordering::SeqCst);
        let (_, body) = get(a, "GET /index.m3u8 HTTP/1.1\r\n\r\n");
        let pl = String::from_utf8(body).unwrap();
        assert!(
            pl.contains("seg-00000002.ts") && pl.contains("#EXT-X-ENDLIST"),
            "{pl}"
        );

        srv.stop();
        assert!(
            TcpStream::connect_timeout(&a, Duration::from_millis(500)).is_err(),
            "port closed after stop"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Dry-run hook (scripts/tests/hls-cast-dryrun.sh): builds the playlist
    /// for a directory of real VLC-written segments and checks it.
    #[test]
    #[ignore]
    fn real_segment_dir_gives_a_valid_playlist() {
        let Ok(dir) = std::env::var("NEBULAE_HLS_DIR") else {
            return;
        };
        let mut srv =
            LanServer::start(PathBuf::from(&dir), loopback(), 0, Box::new(|| true)).unwrap();
        let (_, body) = get(srv.addr, "GET /index.m3u8 HTTP/1.1\r\n\r\n");
        let pl = String::from_utf8(body).unwrap();
        let durs: Vec<f64> = pl
            .lines()
            .filter_map(|l| l.strip_prefix("#EXTINF:"))
            .filter_map(|l| l.trim_end_matches(',').parse().ok())
            .collect();
        // The playlist the server returned, for an independent ffmpeg decode.
        let _ = std::fs::write(Path::new(&dir).join("served.m3u8"), &pl);
        let max = durs.iter().cloned().fold(0.0, f64::max);
        let discs = pl.matches("#EXT-X-DISCONTINUITY").count();
        println!(
            "HLS_REAL segments={} total_s={:.1} min={:.2} max={:.2} discontinuities={} endlist={}",
            durs.len(),
            durs.iter().sum::<f64>(),
            durs.iter().cloned().fold(f64::MAX, f64::min),
            max,
            discs,
            pl.contains("ENDLIST")
        );
        assert!(
            !durs.is_empty() && durs.iter().all(|d| *d > 0.0),
            "positive durations"
        );
        srv.stop();
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_segment_is_not_served() {
        let dir = tmpdir("link");
        std::fs::write(dir.join("target.bin"), "secret").unwrap();
        std::os::unix::fs::symlink(dir.join("target.bin"), dir.join("seg-00000000.ts")).unwrap();
        let mut srv = LanServer::start(dir.clone(), loopback(), 0, Box::new(|| false)).unwrap();
        let (h, _) = get(srv.addr, "GET /seg-00000000.ts HTTP/1.1\r\n\r\n");
        assert!(h.starts_with("HTTP/1.1 404"), "{h}");
        srv.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
