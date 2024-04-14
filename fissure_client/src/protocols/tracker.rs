use crate::bee_processor::bee_decoder::BeeDecoderErr;
use crate::models::client_meta::ClientTorrentMetaInfo;
use crate::models::torrent_meta::{TrackerRequest, TrackerResponse};
use reqwest;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct TrackerRequestErr {
    action: String,
    error_string: String,
}

impl fmt::Display for TrackerRequestErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "action : {:?} reason : {:?}",
            self.action, self.error_string
        )
    }
}

impl Error for TrackerRequestErr {}
// Needs rafactor to be able to run with only ClientState and for all torrents
pub async fn refresh_peer_list_from_tracker(
    client_torrent_meta_info_arc_mutex: &Arc<RwLock<ClientTorrentMetaInfo>>,
    peer_id: String,
    port: String,
) -> Result<TrackerResponse, BeeDecoderErr> {
    let client_torrent_meta_info = client_torrent_meta_info_arc_mutex.read().await;
    let req = TrackerRequest {
        info_hash: client_torrent_meta_info.info_hash,
        peer_id,
        port,
        uploaded: client_torrent_meta_info.uploaded.clone(),
        downloaded: client_torrent_meta_info.downloaded.clone(),
        left: client_torrent_meta_info.left.clone(),
    };
    let qs = req.generate_query_string();
    let req_client = reqwest::Client::new();

    let url_with_parameters = format!("{}?{}", client_torrent_meta_info.raw_torrent.announce, qs);
    // Needs to be debug
    println!("Making request to tracker : {}", url_with_parameters);

    let res = req_client
        .get(url_with_parameters)
        .send()
        .await
        .expect("Failed to make connection :(. Check your internet connection.");
    let raw_body = res
        .text_with_charset("WINDOWS-1252")
        .await
        .expect("Error opening body from response, maybe connection got interrupted.");
    println!("here");
    let resp = TrackerResponse::from_raw_text_response_body(raw_body);
    println!("{:?}", resp);
    return resp;

    //mutex is dropped after scope
}
