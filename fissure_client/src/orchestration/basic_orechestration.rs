use crate::models::client_meta::ClientState;
use crate::orchestration::handshake_orechestration;
use crate::orchestration::torrent_refresh;
use crate::protocols;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

pub async fn start_dowload(client_state: Arc<RwLock<ClientState>>, torrent_file_path: &str) {
    let already_present_torrents = client_state.read().await.torrents.len();
    client_state
        .write()
        .await
        .add_torrent_using_file_path(torrent_file_path.to_string());
    //Response from trackers cannot have more than 300 peers
    println!("{}", client_state.read().await.torrents.len());
    let (peer_tracker_handshake_channel_tx, peer_tracker_handshake_channel_rx) = mpsc::channel(300);

    let inner_arc = client_state.clone(); // Arc clone
    tokio::spawn(async move {
        torrent_refresh::torrent_refresh(
            already_present_torrents,
            inner_arc,
            peer_tracker_handshake_channel_tx,
        )
        .await;
    });

    tokio::spawn(async move {
        handshake_orechestration::handshake_orchestrator(
            peer_tracker_handshake_channel_rx,
            already_present_torrents,
            Arc::clone(&client_state),
        )
        .await;
    });
    loop { //[BAD CODE] Need to use waitgroups
        //psueod await, lmao very debuggy bad code
    }
}
