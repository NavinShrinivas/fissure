use crate::models::client_meta::ClientState;
use crate::models::client_meta::ClientTorrentMeta;
use crate::models::torrent_meta::{TrackerReponse, TrackerRequest};
use reqwest;
use std::error::Error;
use std::fmt;

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
    client_torrent_meta: &ClientTorrentMeta,
    client_state: &ClientState,
) -> Result<TrackerReponse, TrackerRequestErr> {
    let req = TrackerRequest {
        info_hash: client_torrent_meta.info_hash,
        peer_id: client_state.peer_id.clone(),
        port: client_state.port.clone(),
        uploaded: client_torrent_meta.uploaded.clone(),
        downloaded: client_torrent_meta.downloaded.clone(),
        left: client_torrent_meta.left.clone(),
    };
    let qs = req.generate_query_string();
    let client = reqwest::Client::new();

    let url_with_parmeters = format!("{}?{}", client_torrent_meta.raw_torrent.announce, qs);
    // Needs to be debug
    println!("Making request to tracker : {}", url_with_parmeters);
    let res = client
        .get(url_with_parmeters)
        .send()
        .await
        .expect("Failed to make connection :(. Check your internet connection.");
    let raw_body = res
        .text_with_charset("WINDOWS-1252")
        .await
        .expect("Error opening body from response, maybe connection got interrupted.");

    let tracker_response = TrackerReponse::from_raw_text_response_body(raw_body);
    return Ok(tracker_response.unwrap());

}
