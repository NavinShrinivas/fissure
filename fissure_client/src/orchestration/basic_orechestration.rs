use crate::models::client_meta::ClientState;
use crate::orchestration::handshake_orechestration;
use crate::orchestration::torrent_refresh;
use async_channel;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;

/// This function also takes care of work allocation, hence MPMC
pub async fn start_dowload(client_state: Arc<RwLock<ClientState>>, torrent_file_path: &str) {
    let already_present_torrents = client_state.read().await.torrents.len();
    client_state
        .write()
        .await
        .add_torrent_using_file_path(torrent_file_path.to_string());

    println!("{}", client_state.read().await.torrents.len());
    //Response from trackers cannot have more than 300 peers
    let (peer_tracker_handshake_channel_tx, peer_tracker_handshake_channel_rx) = mpsc::channel(300);

    // A given torrent file cannot have more than 300 new connection any given time
    // We send the peer_connection type to job sched so that we can make decision based on data
    // let (job_sched_peer_connection_tx , job_sched_peer_connection_rx) = mpsc::channel(300); /
    // For future

    let inner_arc = Arc::clone(&client_state);
    tokio::spawn(async move {
        torrent_refresh::torrent_refresh(
            already_present_torrents,
            inner_arc,
            peer_tracker_handshake_channel_tx,
        )
        .await;
    });

    tokio::spawn(async move {
        // Handshake Orchestration initiates connect and spawns a thread for each connection on a
        // State Machine type processor with input from MPMC consumer side.
        // MPMC sender side need to stay in this function
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
