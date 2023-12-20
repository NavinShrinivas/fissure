use crate::models::client_meta::ClientState;
use crate::models::torrent_jobs;
use crate::models::torrent_meta::TrackerReponse;
use crate::orchestration::handshake_orechestration;
use crate::orchestration::job_orchestrator;
use crate::orchestration::torrent_refresh;
use crossbeam_channel;
use std::sync::Arc;
use std::{thread, time};
use tokio::sync::RwLock;

/// This function does the following in this mentioned order :
///
pub async fn start_dowload(client_state: Arc<RwLock<ClientState>>, torrent_file_path: &str) {
    // Needed as even after we add more torrent to the client state, we want the thread to continue
    // working on the same torrent they were
    let current_torrent_to_work_on_index = client_state.read().await.torrents.len();
    client_state
        .write()
        .await
        .add_torrent_using_file_path(torrent_file_path.to_string());

    // println!("debug : {}", client_state.read().await.torrents.len()); //[DEBUG]

    //Response from trackers cannot have more than 300 peers, this channel is used to send "new
    //peers" to handshake_orchestrator (tracker module -> handshake_module)
    let (peer_tracker_handshake_channel_tx, peer_tracker_handshake_channel_rx) = crossbeam_channel::bounded::<TrackerReponse>(300);

    //[TODO]
    // let (job_sched_peer_connection_tx , job_sched_peer_connection_rx) = mpsc::channel(300); /

    let client_state_tracker_refresh = Arc::clone(&client_state);
    let client_state_handshake = Arc::clone(&client_state);
    let client_state_job_orchestrator = Arc::clone(&client_state);
    tokio::spawn(async move {
        torrent_refresh::torrent_refresh(
            current_torrent_to_work_on_index,
            client_state_tracker_refresh,
            peer_tracker_handshake_channel_tx,
        )
        .await;
    });

    let (unfinished_job_snd, unfinished_job_recv) =
        crossbeam_channel::bounded::<torrent_jobs::Job>(2000);

    //[TODO]
    // let (finised_job_snd, finished_job_recv) = crossbeam_channel::unbounded::<torrent_jobs::Job>();

    let (unfinished_job_snd_handshake, unfinished_job_recv_handshake) =
        (unfinished_job_snd.clone(), unfinished_job_recv.clone());
    let unfinished_job_snd_job_orchestrator = unfinished_job_snd.clone();
    tokio::spawn(async move {
        //handshake_orchestrator first initates a connection to each peer from peer_tracker channel
        //and spawns a peer_protcol state machine for each connection, where each state machine
        //needs a unfinished_job send and recv
        //[TODO] It will also need finished_job send to send the pieces to file assembler
        handshake_orechestration::handshake_orchestrator(
            peer_tracker_handshake_channel_rx,
            current_torrent_to_work_on_index,
            client_state_handshake,
            unfinished_job_snd_handshake,
            unfinished_job_recv_handshake,
        )
        .await;
    });

    // thread::sleep(time::Duration::from_secs(5)); // trying to avoid race between write and read
                                                 // lock on client state, HORRIBLE CODING...I HOPE
                                                 // TO GOD I FIX THIS PROPERLY.
                                                 //Simply put we dont want the send sink to start be invoked
                                                 //by job orchestrator before atl least one recv is
                                                 //ready from the peer_protocol recv, but in mot
                                                 //100% sure tho
    tokio::spawn(async move {
        job_orchestrator::job_orchestrator(
            unfinished_job_snd_job_orchestrator,
            current_torrent_to_work_on_index,
            client_state_job_orchestrator,
        )
        .await
    });
    loop { //[BAD CODE] Need to use waitgroups
         //psueod await, lmao very debuggy bad code
    }
}
