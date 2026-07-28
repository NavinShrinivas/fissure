use crate::managers::torrent_manager::TorrentManager;
use crate::protocols::tracker::{Peer, TrackerResponse};
use crate::protocols;
use log::{debug, error};
use tokio::sync::RwLock;
use tokio::time::{Sleep, sleep};
use std::sync::Arc;
use std::time;

pub async fn torrent_refresh(
    torrent_manager: TorrentManager,
    peer_id: String,
    port: String,
    peer_sender: tokio::sync::mpsc::Sender<TrackerResponse>,
) {
    log::info!("Starting torrent tracker refresh thread.");
    let c = torrent_manager.clone();
    let accounce_list = c.meta.raw_torrent.announce_list.clone();
    if accounce_list.is_some(){
        log::info!("Using newer announce-list logic");
        let already_seen_peers : Arc<RwLock<Vec<Peer>>> = Arc::new(RwLock::new(Vec::new()));
        let list = accounce_list.clone().unwrap();
        //For each tier we need to spawn a retry loop
        for tier in list {
            let clone_manager= torrent_manager.clone();
            let peer_id_clone = peer_id.clone();
            let port_clone = port.clone();
            let seen_vec = already_seen_peers.clone();
            let sender_clone = peer_sender.clone();
            let mut tier_clone = tier.clone();
            tokio::spawn(async move{
                loop{
                    let mut interval = 0;
                    let mut tier_retry = 0; //backoff retry for each tier
                    for idx in 0..tier_clone.len() {
                        let url = tier[idx].clone();
                        let inner_clone = clone_manager.clone();
                        let inner_peer_id = peer_id_clone.clone();
                        let inner_port = port_clone.clone();
                        let inner_sender = sender_clone.clone();

                        let tracker_res;

                        if url.starts_with("https") || url.starts_with("http"){
                            // tracker_res = protocols::tracker::refresh_peer_list_from_http_trackers(
                            //     inner_clone, url.clone(), inner_peer_id, inner_port).await;
                            todo!();
                        }else{
                            tracker_res = protocols::tracker::refresh_peer_list_from_udp_tracker(
                                inner_clone, url.clone(), inner_peer_id, inner_port, tier_retry).await;
                            log::debug!("respohse parsed : {:?}", tracker_res);
                        }


                        let mut new_resp = match tracker_res{
                            Ok(k) => k,
                            Err(e) => {
                                log::error!("Error making call to tracker : {}", e);
                                tier_retry += 1;
                                continue;
                            }
                        };
                        let mut new_resp_peer = new_resp.peers.unwrap();
                        let read_guard = seen_vec.read().await;

                        new_resp_peer.retain(|f| read_guard.iter().any(|o| o.ip == f.ip) == false);

                        drop(read_guard);

                        let mut write_guard = seen_vec.write().await;
                        write_guard.extend(new_resp_peer.clone());

                        interval = new_resp.interval.unwrap();
                        new_resp.peers = Some(new_resp_peer);
                        inner_sender.send(new_resp).await.unwrap();
                        tier_clone.remove(idx);
                        tier_clone.insert(0, url.clone());
                        break;

                    }
                    sleep(time::Duration::from_secs(if interval != 0 {interval as u64} else {(5u32).pow(tier_retry) as u64})).await;
                }
            });
        }

    }else{
        let mut old_tracker_response: Option<TrackerResponse> = None;
        log::info!("Using legacy accounce url field for tracker. Only single tracker.");
        let url = &torrent_manager.clone().meta.raw_torrent.announce;
        loop {
            let manager_handle = torrent_manager.clone();
            
            let tracker_result = protocols::tracker::refresh_peer_list_from_http_trackers(
                manager_handle,
                url.clone(),
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
}