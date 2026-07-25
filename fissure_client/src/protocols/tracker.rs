use crate::bee_processor::bee_decoder::FissureErr;
use crate::managers::torrent_manager::TorrentManager;
use log::debug;
use reqwest;
use serde::{Deserialize, Serialize};
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
    torrent_manager: TorrentManager,
    peer_id: String,
    port: String,
) -> Result<TrackerResponse, FissureErr> {
    let torrent_stats = match torrent_manager.get_torrent_stats().await{
        Some( stats) => stats, 
        None => {
            return Err(FissureErr::new("Got none for torrent stats before making tracker call".to_string()));
        }
    };
    let req = TrackerRequest {
        info_hash: torrent_manager.meta.info_hash,
        peer_id,
        port,
        uploaded: torrent_stats.uploaded.clone(),
        downloaded: torrent_stats.downloaded.clone(),
        left: torrent_stats.left.clone(),
    };
    let qs = req.generate_query_string();
    let req_client = reqwest::Client::new();

    let url_with_parameters = format!("{}?{}", torrent_manager.meta.raw_torrent.announce, qs);
    // Needs to be debug
    debug!("Making request to tracker : {}", url_with_parameters);

    let res = req_client
        .get(url_with_parameters)
        .send()
        .await
        .expect("Failed to make connection :(. Check your internet connection.");
    let raw_body = res
        .text_with_charset("WINDOWS-1252")
        .await
        .expect("Error opening body from response, maybe connection got interrupted.");
    let resp = TrackerResponse::from_raw_text_response_body(raw_body);
    debug!("{:?}", resp);
    return resp;
}

//===================tracker comms==================
/*
  #[serde(alias = "name")]
    Deserialize this field from the given name or from its Rust name. May be repeated to specify multiple possible names for the same field.
*/
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    #[serde(alias = "peer id", alias = "peer_id")]
    pub peer_id: Option<String>,
    pub ip: String, //Can be ipv4, ipv6 or domain name. need to parse that later.
    pub port: i32,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrackerResponse {
    #[serde(alias = "failure reason", alias = "failure_reason")]
    pub failure_reason: Option<String>,
    pub interval: Option<i64>,
    pub peers: Option<Vec<Peer>>,
}

impl TrackerResponse {
    pub fn from_raw_text_response_body(raw_text: String) -> Result<TrackerResponse, FissureErr> {
        match serde_bencoded::from_str(&raw_text.to_string()) {
            Ok(res) => Ok(res),
            Err(e) => Err(FissureErr::new(
                "[ERROR] Error decoding tracker response from bencoding.".to_string()
                    + &e.to_string(),
            )),
        }
    }
}

pub struct TrackerRequest {
    // We can form querystring from either binary or string.
    pub info_hash: [u8; 20],
    pub peer_id: String,
    pub port: String,
    pub uploaded: String,   //Base10 ASCII
    pub downloaded: String, //Base10 ASCII
    pub left: String,       //Base10 ASCII
}
impl TrackerRequest {
    pub fn generate_query_string(&self) -> String {
        let mut t_string: String;
        t_string = format!(
            "info_hash={}",
            urlencoding::encode_binary(self.info_hash.as_slice())
        );

        t_string = format!(
            "{}&peer_id={}",
            t_string,
            urlencoding::encode(&self.peer_id)
        );

        t_string = format!("{}&port={}", t_string, urlencoding::encode(&self.port));
        t_string = format!(
            "{}&uploaded={}",
            t_string,
            urlencoding::encode(&self.uploaded)
        );

        t_string = format!(
            "{}&downloaded={}",
            t_string,
            urlencoding::encode(&self.downloaded)
        );

        t_string = format!("{}&left={}", t_string, urlencoding::encode(&self.left));

        return t_string;
    }
}

//==================================================