use crate::bee_processor::bee_decoder::FissureErr;
use crate::managers::torrent_manager::TorrentManager;
use log::debug;
use reqwest;
use serde::{Deserialize, Deserializer};
use std::net::Ipv4Addr;
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
        uploaded: ((torrent_stats.uploaded.clone().parse::<f64>().unwrap() * 1000000 as f64) as u64).to_string(),
        downloaded: ((torrent_stats.downloaded.clone().parse::<f64>().unwrap() * 1000000 as f64) as u64).to_string(),
        left: ((torrent_stats.left.clone().parse::<f64>().unwrap() * 1000000 as f64) as u64).to_string(),
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

    // 2. FIX: Extract raw binary bytes directly instead of text
    let raw_bytes = res
        .bytes()
        .await
        .expect("Error opening body from response, maybe connection got interrupted.");
    let resp = TrackerResponse::from_raw_bytes_response_body(&raw_bytes);
    debug!("{:?}", resp);
    return resp;
}

//===================tracker comms==================
/*
  #[serde(alias = "name")]
    Deserialize this field from the given name or from its Rust name. May be repeated to specify multiple possible names for the same field.
*/

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    // FIX: Read the peer id as raw bytes first so Serde never chokes on weird characters,
    // then convert it to a safe lossy String representation automatically.
    #[serde(alias = "peer id", alias = "peer_id", deserialize_with = "deserialize_peer_id", default)]
    pub peer_id: Option<String>,
    pub ip: String, // Safely captures IPv4 or IPv6 text strings (e.g. "2001:bc8...")
    pub port: i32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TrackerResponse {
    #[serde(alias = "failure reason", alias = "failure_reason")]
    pub failure_reason: Option<String>,
    pub interval: Option<i64>,
    
    // Automatically normalizes both compact arrays and the verbose dictionary lists
    #[serde(deserialize_with = "deserialize_peers", default)]
    pub peers: Option<Vec<Peer>>,
}

// 1. Helper to safely deserialize erratic binary peer IDs into strings
fn deserialize_peer_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes = Option::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
    Ok(bytes.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

// 2. Multi-mode tracker response peer normalizer
fn deserialize_peers<'de, D>(deserializer: D) -> Result<Option<Vec<Peer>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawPeers {
        Normal(Vec<Peer>),
        #[serde(with = "serde_bytes")]
        Compact(Vec<u8>),
    }

    let raw = Option::<RawPeers>::deserialize(deserializer)?;
    
    match raw {
        None => Ok(None),
        Some(RawPeers::Normal(list)) => Ok(Some(list)),
        Some(RawPeers::Compact(bytes)) => {
            let mut parsed_peers = Vec::new();
            if bytes.len() % 6 == 0 {
                for chunk in bytes.chunks_exact(6) {
                    let ip_addr = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                    let port_num = u16::from_be_bytes([chunk[4], chunk[5]]) as i32;

                    parsed_peers.push(Peer {
                        peer_id: None,
                        ip: ip_addr.to_string(),
                        port: port_num,
                    });
                }
            }
            Ok(Some(parsed_peers))
        }
    }
}

impl TrackerResponse {
    pub fn from_raw_bytes_response_body(raw_bytes: &[u8]) -> Result<TrackerResponse, FissureErr> {
        match serde_bencoded::from_bytes(raw_bytes) {
            Ok(res) => Ok(res),
            Err(e) => {
                log::info!("{:?}", raw_bytes);
                Err(FissureErr::new(
                "[ERROR] Error decoding tracker response from bencoding. res:".to_string()
                    + &e.to_string(), 
            ))
        }
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