//! Probe: chapter offsets straight from the disc playlists (libbluray).
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/Volumes/300TB/Blu-ray/Fuuka (2017) Funimation/FUUKA_CS_D1".into());
    let dur: i64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    println!("Testing path: {path}, dur: {dur}");
    match nebulae_torrent_client_lib::player::bluray_meta::disc_chapter_offsets(&path, dur) {
        Ok((o, d)) => println!("current disc_chapter_offsets ({}): {:?}, playlist_dur: {}", o.len(), o, d),
        Err(e) => println!("ERR: {e}"),
    }
}
