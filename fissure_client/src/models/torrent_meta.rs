/*!
    decodes the bencoded .torrent files and splits them into the properties
    defined in the initial bit torrent protocol (https://www.bittorrent.org/beps/bep_0003.html)
*/

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use urlencoding;
// If I'd have to match to a different name
// #[serde(rename = "piece length")]

//===================torrent file==================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileInfo {
    pub length: u64,       // Size of each file
    pub path: Vec<String>, // Path of the file, not sure if relative or not. Should be parseable by PathBuf
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Info {
    //In the single file case, the name key is the name of a file, in the multiple file case, it's the name of a directory.
    pub name: String, // The file name/path to store...only recommended
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaInfo {
    pub announce: String, // Contains the url for the tracker
    pub info: Info,
}

impl MetaInfo {
    pub fn files(&self) -> Vec<FileInfo> {
        let files = if self.info.length.is_some() {
            vec![FileInfo {
                length: self.info.length.unwrap(),
                path: vec![self.info.name.clone()],
            }]
        } else {
            self.info.files.clone().unwrap()
        };
        return files;
    }
    pub fn print_files(&self) {
        let files = self.files();
        for (index, content) in files.iter().enumerate() {
            let path_buf: PathBuf = content.path.iter().collect();
            println!(
                "\t\t {}. path : {:?}, size :{} MB",
                index + 1,
                path_buf,
                content.length / 1000000
            );
        }
    }
    pub fn download_size(&self) -> u64 {
        let mut tot_size: u64 = 0;
        for i in self.files() {
            tot_size += i.length;
        }
        return tot_size;
    }
}
//==================================================

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
