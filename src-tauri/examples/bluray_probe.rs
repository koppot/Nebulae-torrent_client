use nebulae_torrent_client_lib::player::engine::{VlcLib, VlcSession};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    let lib = VlcLib::load().expect("load vlc");
    println!("vlc version: {}", lib.version());
    let path = std::env::args().nth(1).expect("disc root argument");
    let session = match VlcSession::open(&lib, &path, 0) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("open failed: {e}");
            return;
        }
    };
    for i in 0..6 {
        sleep(Duration::from_millis(1000));
        unsafe {
            let tc = (lib.libvlc_media_player_get_title_count)(session.media_player as *mut _);
            let t = (lib.libvlc_media_player_get_title)(session.media_player as *mut _);
            let tm = (lib.libvlc_media_player_get_time)(session.media_player as *mut _);
            let len = (lib.libvlc_media_player_get_length)(session.media_player as *mut _);
            println!("t={i} titles={tc} title={t} time={tm} len={len}");
        }
    }
    session.close(&lib);
}
