use std::thread::sleep;
use std::time::Duration;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::managers::client_manager::FissureClientOptions;
use crate::managers::torrent_manager::TorrentManager;

pub fn generate_peer_id(client_env: &FissureClientOptions) -> String {
    let id = "FS";
    let rand_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(14)
        .map(char::from)
        .collect();
    let id = format!("{}{}{}", id, client_env.version, rand_string);
    log::debug!("Our peer id : {}", id);
    return id.to_string();
}

pub async fn timed_torrent_stats_logger(torrent_manager: TorrentManager) {
    sleep(Duration::from_secs(15));
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        ticker.tick().await;
        let stats = torrent_manager.get_torrent_stats().await.unwrap();
        let active_peers = torrent_manager.get_active_peers().await.unwrap();
        log::info!(
            "Download Stats : Downloaded : {} MB, Uploaded : {} MB, Left : {} MB",
            stats.downloaded ,
            stats.uploaded ,
            stats.left
        );
        log::info!(
            "Active Peers : {}",
            active_peers.len()
        );
        // log::info!(
        //     "Active Pieces : {}",
        //     torrent_manager.active_pieces_manager.len()
        // );
    }
}
