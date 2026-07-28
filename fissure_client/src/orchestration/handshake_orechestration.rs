
use crate::protocols::tracker::TrackerResponse;
use crate::{managers::torrent_manager::TorrentManager, protocols::peer_handshake::PeerConnection};
use crate::protocols::peer_protocol_state_machine;
use log::{debug, error};
use std::{time::Duration};
use tokio::time::sleep;

pub async fn handshake_orchestrator(
    mut peers_rs: tokio::sync::mpsc::Receiver<TrackerResponse>,
    torrent_manager: TorrentManager,
    our_peer_id: String
) {
    //The channel only sends new peer, we do handshake get back the PeerConnection and spawn a
    //peer protcol thread, maintaining a thread pool
    // thread::sleep(time::Duration::from_secs(5)); //I have no clue why this is working, if the recv
    //starts before send silent fail
    //[BAD CODE] Need to somehow time its starting
    //with recv

    log::info!("Starting handshake_orchestrator");
    let mut retires = 10;
    while retires > 0 {
        log::info!("Waiting to receive new peers from tracker...");
            let delta_tracker_response = match peers_rs.recv().await {
                Some(resp) => {
                    log::debug!("Received new peer info from trackers : {:?}", resp);
                    retires = 10;
                    resp
                },
                None => {
                        error!("Timeout receiving peer info from trackers, retrying handhsake routine. err.");
                        retires = retires - 1;
                        sleep(Duration::from_secs_f32(2.0)).await;
                        continue;
                    }
            };
        log::info!("handshake_orchestrator: after recv await");

        let new_peer_list = match delta_tracker_response.peers {
            Some(x) => x,
            None => {
                debug!("Receivied no new peers, retrying in a while.");
                continue;
            }
        };
        for i in new_peer_list {
            let manager_clone = torrent_manager.clone();
            let our_peer_id_clone = our_peer_id.clone();
            tokio::spawn(async move {
                let peer_connection = PeerConnection::peer_connection_from_peer_meta(
                    &i,
                    manager_clone,
                    our_peer_id_clone
                )
                .await;
                match peer_connection{
                    Some(c) => {
                        peer_protocol_state_machine::state_machine(
                            c
                        ).await;
                    }, 
                    None => {
                    }
                }
            });
        }
    }
}
