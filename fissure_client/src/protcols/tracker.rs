use crate::models::client_meta::ClientState;
use crate::models::torrent_meta::MetaInfo;
use crate::models::torrent_meta::{TrackerReponse, TrackerRequest};
use reqwest;
use serde_bencoded;
use serde_json;
use serde_qs;
use sha1::{Digest, Sha1};
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
    torrent_meta: &MetaInfo,
    client_state: &ClientState,
) -> Result<TrackerReponse, TrackerRequestErr> {
    // create a Sha1 object
    let mut hasher = Sha1::new();

    // process input message
    let info_value_bencoded  = match serde_bencoded::to_string(&torrent_meta.info){
        Ok(v) => {v},
        Err(e) => {
            return Err(TrackerRequestErr{
                action : "get".to_string(),
                error_string : "Error encoding info value back to bencoded to calculate hash before tracker request.".to_string()+&e.to_string()
            })
        }
    };

    // acquire hash digest in the form of GenericArray,
    // which in this case is equivalent to [u8; 20], sha-1 is alwyas 20 bytes.
    hasher.update(info_value_bencoded);
    let info_hash = hasher.finalize();
    let mut info_hash_escaped = String::new();
    println!("{:?}", String::from_utf8(info_hash.to_vec()));
    for i in info_hash.iter() {
        info_hash_escaped = format!("{}\\x{}", info_hash_escaped, i.to_string());
    }

    let req = TrackerRequest {
        info_hash: info_hash_escaped,
        peer_id: client_state.peer_id.clone(),
        port: client_state.port.clone(),
        uploaded: "0".to_string(),
        downloaded: "0".to_string(),
        left: torrent_meta.info.length.unwrap().to_string(), //Will break if more than 1 files
    };
    let client = reqwest::Client::new();
    let paramters = match serde_qs::to_string(&req) {
        Ok(str) => str,
        Err(e) => {
            return Err(TrackerRequestErr {
                action: "get".to_string(),
                error_string: "Error parsing request paramters to query string.".to_string()
                    + &e.to_string(),
            })
        }
    };
    let url_with_parmeters = format!("{}?{}", torrent_meta.announce, paramters);
    println!("{}", url_with_parmeters);
    let res = client.get(url_with_parmeters).send().await;
    let res_body: TrackerReponse =
        serde_json::de::from_str(res.unwrap().text().await.unwrap().as_str()).unwrap();
    return Ok(res_body);
}
