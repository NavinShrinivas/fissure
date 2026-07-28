use crate::bee_processor::bee_decoder::FissureErr;
use crate::managers::torrent_manager::TorrentManager;
use log::debug;
use reqwest;
use serde::{Deserialize, Deserializer};
use tokio::net::UdpSocket;
use tokio::time::timeout;
use url::Url;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::error::Error;
use std::fmt;
use std::os::macos::raw::stat;
use std::time::Duration;

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
pub async fn refresh_peer_list_from_http_trackers(
    torrent_manager: TorrentManager,
    url: String,
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
    let req_client = reqwest::Client::builder().user_agent("qBittorrent/4.5.2").build().unwrap();
    let separator = if url.contains('?') { "&" } else { "?" };
    let url_with_parameters = format!("{}{}{}", url, separator, qs);
    log::info!("{:?}", url_with_parameters);
    // Needs to be debug
    debug!("Making request to tracker : {}", url_with_parameters);

    let res = match req_client
        .get(url_with_parameters)
        .send()
        .await{
            Ok(r) => r,
            Err(e) => {
                log::error!("Error makign tracker request : {:?}", e);
                return Err(FissureErr::new(e.to_string()));
            }
        };

    // 2. FIX: Extract raw binary bytes directly instead of text
    let raw_bytes = res
        .bytes()
        .await
        .expect("Error opening body from response, maybe connection got interrupted.");
    let resp = TrackerResponse::from_raw_bytes_response_body(&raw_bytes);
    debug!("{:?}", resp);
    return resp;
}

pub async fn refresh_peer_list_from_udp_tracker(
    torrent_manager: TorrentManager,
    url: String, 
    our_peer_id: String, 
    our_port: String,
    retry: u32,
) -> Result<TrackerResponse, FissureErr>{
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(&extract_tracker_host_port(&url).unwrap()).await;
    
    //first packet : 
    let t_id: u32 = rand::random();
    let magic : u64 = 0x41727101980;
    let action : u32= 0;
    let mut first_request_buf : Vec<u8> = Vec::with_capacity(16);
    first_request_buf.extend_from_slice(&magic.to_be_bytes());
    first_request_buf.extend_from_slice(&action.to_be_bytes());
    first_request_buf.extend_from_slice(&t_id.to_be_bytes());
    sock.send(&first_request_buf).await;

    let mut connect_buf = [0u8; 16];

    let c_id = match timeout(Duration::from_secs(15 * (2u64.pow(retry) + 1)), sock.recv(&mut connect_buf)).await{
        Ok(Ok(first_res)) => {
            let res_action = u32::from_be_bytes(connect_buf[0..4].try_into().unwrap());
            let r_t_id = u32::from_be_bytes(connect_buf[4..8].try_into().unwrap());
            let c_id = u64::from_be_bytes(connect_buf[8..16].try_into().unwrap());
            if res_action !=0 || r_t_id != t_id {
                return Err(FissureErr::new("Malcious UDP tracker messing with stuff in first req".to_string()));
            }
            c_id

        }
        Ok(Err(err)) => {
            return Err(FissureErr::new(err.to_string()));
        }
        Err(_) => {
            return Err(FissureErr::new("Timeout for first UDP request".to_string()));
        }
    };

    //second request ; 

    let mut second_request_buf: Vec<u8> = Vec::new();
    let action_announce: u32 = 1;
    let event_started: u32 = 2; 
    let key: u32 = rand::random();
    let num_want: i32 = -1; 
    let stats = torrent_manager.clone().get_torrent_stats().await.unwrap();
    let t_id: u32 = rand::random();
    second_request_buf.extend_from_slice(&c_id.to_be_bytes());
    second_request_buf.extend_from_slice(&action_announce.to_be_bytes());
    second_request_buf.extend_from_slice(&t_id.to_be_bytes());
    second_request_buf.extend_from_slice(&torrent_manager.clone().meta.info_hash);
    second_request_buf.extend_from_slice(our_peer_id.as_bytes());
    second_request_buf.extend_from_slice(&((stats.downloaded.parse::<f64>().unwrap() * 1000000 as f64) as u64).to_be_bytes());
    second_request_buf.extend_from_slice(&((stats.left.parse::<f64>().unwrap() * 1000000 as f64) as u64).to_be_bytes());
    second_request_buf.extend_from_slice(&((stats.uploaded.parse::<f64>().unwrap() * 1000000 as f64) as u64).to_be_bytes());
    second_request_buf.extend_from_slice(&event_started.to_be_bytes());
    second_request_buf.extend_from_slice(&0u32.to_be_bytes()); // IP address (0 = default client IP)
    second_request_buf.extend_from_slice(&key.to_be_bytes());
    second_request_buf.extend_from_slice(&num_want.to_be_bytes());
    second_request_buf.extend_from_slice(&our_port.parse::<u16>().unwrap().to_be_bytes());
    log::debug!("Second request : {:?}", second_request_buf);
    sock.send(&second_request_buf).await;
    let mut resp_buf = [0u8; 1024]; 
    match timeout(Duration::from_secs(15 * (2u64.pow(retry) + 1)), sock.recv(&mut resp_buf)).await{
        Ok(Ok(size)) => {
            if size < 20 {
                log::info!("{}", size);
                return Err(FissureErr::new("Invalid second response from UDP tracker".to_string()));
            }
            let res_action = u32::from_be_bytes(resp_buf[0..4].try_into().unwrap());
            let res_tx_id = u32::from_be_bytes(resp_buf[4..8].try_into().unwrap());

            if res_action != 1 || res_tx_id != t_id {
                log::info!("{}{}", res_action, res_tx_id);
                return Err(FissureErr::new("Invalid second response from UDP tracker".to_string()));
            }
            log::debug!("Second response : {:?}", resp_buf);

            return Ok(TrackerResponse::from_udp_bytes(&resp_buf[..size]).unwrap());
        },
        Ok(Err(err)) => {
            return Err(FissureErr::new(err.to_string()));
        }
        Err(_) => {
            return Err(FissureErr::new("Timeout for first UDP request".to_string()));
        }
    }
}
pub fn extract_tracker_host_port(raw_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let parsed = Url::parse(raw_url)?;

    // Extract the host (e.g. "tracker.opentrackr.org")
    let host = parsed
        .host_str()
        .ok_or("Missing host in tracker URL")?;

    // Extract the port, or fallback to standard default ports
    let port = parsed.port().unwrap_or(match parsed.scheme() {
        "http" => 80,
        "https" => 443,
        _ => 80, // Default UDP trackers often specify explicit ports
    });

    // Format strictly as "host:port"
    Ok(format!("{}:{}", host, port))
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

fn deserialize_peer_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes = Option::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
    Ok(bytes.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

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

    pub fn from_udp_bytes(bytes: &[u8]) -> Result<Self, FissureErr> {
        if bytes.len() < 20 {
            return Err(FissureErr::new("Response buffer too short".into()));
        }

        let interval = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let _leechers = u32::from_be_bytes(bytes[12..16].try_into().unwrap());
        let _seeders = u32::from_be_bytes(bytes[16..20].try_into().unwrap());

        // Slice ONLY the peer bytes
        let peer_bytes = &bytes[20..];

        let peers: Vec<Peer> = peer_bytes
            .chunks_exact(6)
            .filter_map(|chunk| {
                let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
                let port = u16::from_be_bytes([chunk[4], chunk[5]]);

                if ip.is_unspecified() || port == 0 {
                    return None;
                }

                Some(Peer {
                    peer_id: None,
                    ip: ip.to_string(),
                    port: port as i32,
                })
            })
            .collect();

        Ok(TrackerResponse {
            failure_reason: None,
            interval: Some(interval as i64),
            peers: Some(peers),
        })
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
    // FORCE every single byte to be %XX format, no matter what ASCII character it represents!
    let encoded_info_hash: String = self
        .info_hash
        .iter()
        .map(|byte| format!("%{:02X}", byte))
        .collect();

    let encoded_peer_id = urlencoding::encode(&self.peer_id);

    format!(
        "info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact=1&event=started",
        encoded_info_hash,
        encoded_peer_id,
        self.port,
        self.uploaded,
        self.downloaded,
        self.left
    )
}

}

//==================================================