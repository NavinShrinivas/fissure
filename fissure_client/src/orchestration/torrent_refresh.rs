use crate::models::client_meta::{ClientState, ClientTorrentMeta};
use crate::models::torrent_meta::Peer;
use crate::models::torrent_meta::TrackerReponse;
use crate::protocols;
use std::sync::Arc;
use std::{thread, time};
use tokio::sync::mpsc::Sender;
use tokio::sync::RwLock;

pub async fn torrent_refresh(
    working_torrent: usize,
    client_state: Arc<RwLock<ClientState>>,
    peer_sender: Sender<TrackerReponse>,
) {
    // We only ship new peers from this for hanshake
    // Clone needed :

    loop {
        let tracker_response = protocols::tracker::refresh_peer_list_from_tracker(
            working_torrent,
            Arc::clone(&client_state),
        )
        .await;

        let mut cs = client_state.write().await;
        let client_torrent_meta: &mut ClientTorrentMeta = match cs.torrents.get_mut(working_torrent)
        {
            Some(ctm) => ctm,
            None => {
                panic!("Wrong torrent index being fed into orchestrators...");
            }
        };
        if client_torrent_meta.tracker_response.is_none() {
            // println!("{:?}", tracker_response);
            client_torrent_meta.tracker_response = Some(match tracker_response {
                Ok(value) => value,
                Err(e) => {
                    println!("{:?}", e.to_string());
                    return;
                }
            });
            let _ = peer_sender
                .send(match client_torrent_meta.tracker_response.clone() {
                    Some(tracker_response) => tracker_response,
                    None => {
                        println!("Tracker did not return back any response...");
                        return;
                    }
                })
                .await;
        } else {
            let old_peers = match client_torrent_meta.tracker_response.as_ref() {
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
                    println!("New, Tracker did not return back any respons...quitting");
                    println!("{:?}", e.to_string());
                    return;
                }
            };

            let mut new_peer: Vec<Peer> = Vec::new();
            for i in new_resp.iter() {
                if old_peers.contains(&i) == false {
                    new_peer.push(i.clone());
                }
            }

            client_torrent_meta.tracker_response = Some(match tracker_response{
                Ok(v) => v,
                Err(e) =>{
                    println!("New, Tracker did not return back any respons...quitting");
                    println!("{:?}", e.to_string());
                    return;
                }
            });
            let _ = peer_sender
                .send(TrackerReponse {
                    failure_reason: None,
                    interval: None,
                    peers: Some(new_peer),
                })
                .await;
        }

        let secs = match client_torrent_meta.tracker_response.clone() {
            Some(internal) => match internal.interval{
                Some(interval) => interval,
                None => {
                    println!("Tracker did not send back any interval time...defaulting to 900 seconds.");
                    900
                }
            },
            None => {
                println!("Tracker did not return back any response...quitting");
                return;
            }
        };
        println!("Sleeping for  : {} secconds", secs);
        // before sleeping, we need to deref all...scary
        std::mem::drop(cs);
        thread::sleep(time::Duration::from_secs(secs as u64));
    }
}