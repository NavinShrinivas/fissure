/*!
    decodes the bencoded .torrent files and splits them into the properties
    defined in the initial bit torrent protocol (https://www.bittorrent.org/beps/bep_0003.html)
*/

use serde::{Deserialize, Serialize};

// If I'd have to match to a different name
// #[serde(rename = "piece length")]

//===================torrent file==================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileInfo {
    pub length: u64,       // Size of each file
    pub path: Vec<String>, // Path of the file, not sure if relative or not. Should be parseable by PathBuf
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
    //In the single file case, the name key is the name of a file, in the muliple file case, it's the name of a directory.
    pub name: String, // The file name/path to store...only reccomended
    #[serde(rename = "piece length")]
    pub piece_length: u64, //Size of each piece the file is split into
    #[serde(rename = "pieces")]
    #[serde(with = "serde_bytes")]
    pub pieces_hash: Vec<u8>, // SHA-1 of all the piece stiched together, each sha-1 is 20 in length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>, // Exists only for single file downloads, tells length of file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>, // Exists only if multi file downloads
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MetaInfo {
    pub announce: String, // Contains the url for the tracker
    pub info: Info,
    #[serde(alias = "info")]
    pub raw_info: String, // Trying to preserve raw string
}
//==================================================

//===================tracker comms==================
/*
  #[serde(alias = "name")]
    Deserialize this field from the given name or from its Rust name. May be repeated to specify multiple possible names for the same field.
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct Peer {
    #[serde(alias = "peer id", alias = "peer_id")]
    pub peer_id: String,
    pub ip: String, //Can be ipv4, ipv6 or domain name. need to parse that later.
    pub port: i32,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct TrackerReponse {
    #[serde(alias = "failure reason", alias = "failure_reason")]
    pub failure_reason: Option<String>,
    pub interval: Option<i64>,
    pub peers: Option<Vec<Peer>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TrackerRequest {
    pub info_hash: String,
    #[serde(alias = "peer id", alias = "peer_id")]
    pub peer_id: String,
    pub port: String,
    pub uploaded: String,   //Base10 ASCII
    pub downloaded: String, //Base10 ASCII
    pub left: String,       //Base10 ASCII
}
//==================================================
