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
        let client_torrent_meta: &mut ClientTorrentMeta =
            cs.torrents.get_mut(working_torrent).unwrap();
        if client_torrent_meta.tracker_response.is_none() {

            // println!("{:?}", tracker_response);
            client_torrent_meta.tracker_response = Some(tracker_response.unwrap());
            peer_sender.send(client_torrent_meta.tracker_response.clone().unwrap()).await;

        } else {
            let old_peers = client_torrent_meta
                .tracker_response
                .as_ref()
                .unwrap()
                .peers
                .clone()
                .unwrap();
            let new_resp = tracker_response.as_ref().unwrap().peers.clone().unwrap();
            let mut new_peer: Vec<Peer> = Vec::new();
            for i in new_resp.iter() {
                if old_peers.contains(&i) == false {
                    new_peer.push(i.clone());
                }
            }

            client_torrent_meta.tracker_response = Some(tracker_response.unwrap());
            peer_sender
                .send(TrackerReponse {
                    failure_reason: None,
                    interval: None,
                    peers: Some(new_peer),
                })
                .await;
        }

        let secs = match client_torrent_meta.tracker_response.clone() {
            Some(internal) => internal.interval.unwrap(),
            None => {
                panic!("Tracker did not return back any response...")
            }
        };
        println!("Sleeping for  : {} secconds", secs);
        // before sleeping, we need to deref all...scary
        std::mem::drop(cs);
        thread::sleep(time::Duration::from_secs(secs as u64));
    }
}
