use tokio::sync::RwLock;

use crate::models::client_meta::ClientTorrentMetaInfo;
use crate::models::torrent_jobs;
use crate::models::torrent_meta::TrackerResponse;
use crate::protocols::peer_handshake::PeerConnection;
use crate::protocols::peer_protocol_state_machine;
use log::{debug, error, info};
use std::{sync::Arc, thread::sleep, time::Duration};

pub async fn handshake_orchestrator(
    peers_rs: crossbeam_channel::Receiver<TrackerResponse>,
    client_torrent_meta_info_arc_mutex: Arc<RwLock<ClientTorrentMetaInfo>>,
    peer_id: &str,
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
    unfinished_job_recv: crossbeam_channel::Receiver<torrent_jobs::Job>,
) {
    //The channel only sends new peer, we do handshake get back the PeerConnection and spawn a
    //peer protcol thread, maintaining a thread pool
    // thread::sleep(time::Duration::from_secs(5)); //I have no clue why this is working, if the recv
    //starts before send silent fail
    //[BAD CODE] Need to somehow time its starting
    //with recv

    debug!("Starting handshake_orchestrator");
    let mut retires = 10;
    while retires > 0 {
        let delta_tracker_response = match peers_rs.recv() {
            Ok(resp) => resp,
            Err(err) => {
                error!("Timeout receiving peer info from trackers, retrying handhsake routine. err : {}", err);
                retires = retires - 1;
                sleep(Duration::from_secs_f32(2.0));
                continue;
            }
        };

        let new_peer_list = match delta_tracker_response.peers {
            Some(x) => x,
            None => {
                debug!("Receivied no new peers, retrying in a while.");
                continue;
            }
        };
        //We have a MPMC channel to communicate between workers thread and the job scheduler.
        for i in new_peer_list {
            let ctmi_inner_clone = client_torrent_meta_info_arc_mutex.clone();
            let peer_id_inner = peer_id.to_string();
            let (s, r) = (unfinished_job_snd.clone(), unfinished_job_recv.clone());
            debug!("Spawning new thread for new peer : {} ", peer_id_inner);
            tokio::spawn(async move {
                let peer_connection_inner = PeerConnection::peer_connection_from_peer_meta(
                    &i,
                    ctmi_inner_clone,
                    peer_id_inner,
                )
                .await;
                peer_protocol_state_machine::state_machine(peer_connection_inner, r, s);
                // We need to spawn peer_protocol thread on the above peer_connection_inner and
                // provide it a job recv of MPMC
            });
        }
    }
}
