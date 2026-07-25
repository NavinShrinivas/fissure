use crate::managers::torrent_manager::TorrentManager;
use crate::protocols::tracker::{Peer, TrackerResponse};
use crate::protocols;
use log::{debug, error};
use tokio::time::sleep;
use std::time;

pub async fn torrent_refresh(
    torrent_manager: TorrentManager,
    peer_id: String,
    port: String,
    peer_sender: tokio::sync::mpsc::Sender<TrackerResponse>,
) {
    let mut old_tracker_response: Option<TrackerResponse> = None;
    log::info!("Starting torrent tracker refresh thread.");

    loop {
        let manager_handle = torrent_manager.clone();
        
        let tracker_result = protocols::tracker::refresh_peer_list_from_tracker(
            manager_handle,
            peer_id.clone(),
            port.clone(),
        ).await;

        let new_resp = match tracker_result {
            Ok(resp) => resp,
            Err(e) => {
                error!("Tracker call failed: {}. Retrying...", e);
                sleep(time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let new_peers = match new_resp.peers.as_ref() {
            Some(p) => p,
            None => {
                debug!("Tracker returned no peers. Retrying...");
                sleep(time::Duration::from_secs(5)).await;
                continue;
            }
        };

        if let Some(old_resp) = &old_tracker_response {
            if let Some(old_peers) = &old_resp.peers {
                let new_only: Vec<Peer> = new_peers.iter()
                    .filter(|p| !old_peers.contains(p))
                    .cloned()
                    .collect();

                if !new_only.is_empty() {
                    let _ = peer_sender.send(TrackerResponse {
                        peers: Some(new_only),
                        ..new_resp.clone()
                    }).await;
                }
            }
        } else {
            let _ = peer_sender.send(new_resp.clone()).await;
        }

        old_tracker_response = Some(new_resp);
        let interval = old_tracker_response.as_ref()
            .and_then(|r| r.interval)
            .unwrap_or(900);

        log::info!("Tracker refresh sleeping for : {} seconds before next refresh.", interval);
        sleep(time::Duration::from_secs(interval as u64)).await;
    }
}