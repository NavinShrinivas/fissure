use crate::models::client_meta::{ClientState, ClientTorrentMeta};
use crate::models::torrent_meta::Peer;
use crate::models::torrent_meta::TrackerReponse;
use crate::protocols;
use crossbeam_channel;
use std::sync::Arc;
use std::{thread, time};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

pub async fn torrent_refresh(
    working_torrent: usize,
    client_state: Arc<RwLock<ClientState>>,
    peer_sender: crossbeam_channel::Sender<TrackerReponse>,
) {
    // We only ship new peers from this for hanshake
    // Clone needed :
    let mut old_tracker_response: Option<TrackerReponse> = None;

    loop {
        let tracker_response = protocols::tracker::refresh_peer_list_from_tracker(
            working_torrent,
            Arc::clone(&client_state),
        )
        .await;

        //Lets skip writing old peers to client_state, as only one thread calculates peers delta
        //and that is this thread, no need to put it in a common state and cause blocking
        // let mut cs = client_state.read().await;
        // println!("OIIIII");
        // let client_torrent_meta: &mut ClientTorrentMeta = match cs.torrents.get_mut(working_torrent)
        // {
        //     Some(ctm) => ctm,
        //     None => {
        //         panic!("Wrong torrent index being fed into orchestrators...");
        //     }
        // };
        // if client_torrent_meta.tracker_response.is_none() {
        //     // println!("{:?}", tracker_response);
        //     client_torrent_meta.tracker_response = Some(match tracker_response {
        //         Ok(value) => value,
        //         Err(e) => {
        //             println!("{:?}", e.to_string());
        //             return;
        //         }
        //     });
        //     let _ = peer_sender.send(match client_torrent_meta.tracker_response.clone() {
        //         Some(tracker_response) => tracker_response,
        //         None => {
        //             println!("Tracker did not return back any response...");
        //             return;
        //         }
        //     });
        if old_tracker_response.is_none() {
            old_tracker_response = match tracker_response {
                Ok(resp) => Some(resp),
                Err(e) => {
                    panic!("Error reading tracker responses to calculate delta : {}", e);
                }
            };
            let _ = peer_sender.send(match old_tracker_response.clone() {
                Some(tracker_response) => tracker_response,
                None => {
                    println!("Tracker did not return back any response...");
                    return;
                }
            });
        } else {
            let old_peers = match old_tracker_response.as_ref() {
                Some(t_resp) => match t_resp.peers.clone() {
                    Some(peers) => peers,
                    None => {
                        println!("Old check, Tracker returned a response, but no peers...qutting");
                        return;
                    }
                },
                None => {
                    println!("Old check, Tracker did not return back any respons...quitting");
                    return;
                }
            };
            let new_resp = match tracker_response.as_ref() {
                Ok(t_resp) => match t_resp.peers.clone() {
                    Some(peers) => peers,
                    None => {
                        println!("New, Tracker returned a response, but no peers...qutting");
                        return;
                    }
                },
                Err(e) => {
                    println!("New, Tracker did not return back any response...quitting");
                    println!("{:?}", e.to_string());
                    return;
                }
            };

            println!("OIIIII");
            let mut new_peer: Vec<Peer> = Vec::new();
            for i in new_resp.iter() {
                if old_peers.contains(&i) == false {
                    new_peer.push(i.clone());
                }
            }

            old_tracker_response = Some(match tracker_response {
                Ok(v) => v,
                Err(e) => {
                    println!("New, Tracker did not return back any respons...quitting");
                    println!("{:?}", e.to_string());
                    return;
                }
            });
            let _ = peer_sender.send(TrackerReponse {
                failure_reason: None,
                interval: None,
                peers: Some(new_peer),
            });
        }

        let secs = match old_tracker_response {
            Some(ref internal) => {
                match internal.interval {
                    Some(interval) => interval,
                    None => {
                        println!("Tracker did not send back any interval time...defaulting to 900 seconds.");
                        900
                    }
                }
            }
            None => {
                println!("Tracker did not return back any response...quitting");
                return;
            }
        };
        println!("Sleeping for  : {} secconds", secs);
        // before sleeping, we need to deref all...scary
        thread::sleep(time::Duration::from_secs(secs as u64));
    }
}
