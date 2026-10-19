// Probe: libbluray track-language lookup for the Fuuka disc at several
// durations (feature playlist, all-disc playlist, menu).
fn main() {
    let path = "/Volumes/300TB/Blu-ray/Fuuka (2017) Funimation/FUUKA_CS_D1";
    for dur in [13_068_000i64, 10_005_000, 1_300_000, 120_000, 0] {
        match nebulae_torrent_client_lib::player::bluray_meta::disc_stream_languages(path, dur) {
            Ok((a, s)) => println!("dur={dur} audio={a:?} subs={s:?}"),
            Err(e) => println!("dur={dur} ERR {e}"),
        }
    }
}
