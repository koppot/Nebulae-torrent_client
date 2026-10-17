fn main() {
    let path = std::env::args().nth(1)
        .unwrap_or_else(|| "/Volumes/300TB/Blu-ray/91 DAYS US/91 DAYS D1".into());
    match nebulae_torrent_client_lib::player::bluray_meta::disc_title_dump(&path) {
        Ok(o) => print!("{o}"),
        Err(e) => println!("ERR: {e}"),
    }
}
