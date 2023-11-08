use crate::models::client_meta::ClientState;
use crate::models::torrent_jobs;
use crate::models::torrent_meta::TrackerReponse;
use crate::protocols::peer_handshake::PeerConnection;
use crate::protocols::peer_protocol_state_machine;
use crossbeam_channel;
use std::sync::Arc;
use std::{thread, time};
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;

pub async fn handshake_orchestrator(
    mut peers_rs: Receiver<TrackerReponse>,
    // mut job_sched_connection_sendder : Sender<Arc<RwLock<PeerConnection>>>, // For future
    working_torret: usize,
    client_state: Arc<RwLock<ClientState>>,
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
    unfinished_job_recv: crossbeam_channel::Receiver<torrent_jobs::Job>,
) {
    //The channel only sends new peer, we do handshake get back the PeeraConnection and spawn a
    //peer protcol thread, maintaining a thread pool
    thread::sleep(time::Duration::from_secs(5)); //I have no clue why this is working, if the recv
                                                 //starts before send silent fail
                                                 //[BAD CODE] Need to somehow time its starting
                                                 //with recv
    loop {
        let delta_tracker_response = match peers_rs.recv().await {
            Some(resp) => resp,
            None => {
                panic!("Send sink closed before recv sink in torrent-handshake channel.");
            }
        };
        //We have a MPMC channel to communicate between workers thread and the job scheduler.
        for i in delta_tracker_response.peers.unwrap() {
            let inner_arc = Arc::clone(&client_state);
            let (s, r) = (unfinished_job_snd.clone(), unfinished_job_recv.clone());
            tokio::spawn(async move {
                let peer_connection_inner =
                    PeerConnection::peer_connection_from_peer_meta(&i, working_torret, inner_arc)
                        .await;
                peer_protocol_state_machine::state_machine(
                    peer_connection_inner,
                    r,
                    s,
                );
                // We need to spawn peer_protocol thread on the above peer_connection_inner and
                // provide it a job recv of MPMC
            });
        }
    }
}
