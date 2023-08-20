use super::torrent_meta::MetaInfo;
use crate::models::torrent_meta::Peer;
use crate::helper;
use crate::ClientEnv;
use sha1::{Digest, Sha1};
use std::path::PathBuf;

/*
  The clinet UI/Interface that needs transformations from
  torrent meta data is stores as ClientTorrentMeta
  The torrent functions still run using torrent_meta_info
*/

pub struct ClientState {
    pub peer_id: String,
    pub torrents: Vec<ClientTorrentMeta>, //Modified transformed torrent meta, used for ease
    pub port: String,
}

impl ClientState {
    pub fn new_client(client_env: ClientEnv) -> ClientState {
        //Peer id format : FS<4digit version><14 random char> : 20 chars long
        ClientState {
            peer_id: helper::generate_peer_id(&client_env),
            torrents: Vec::new(),
            port: client_env.port,
        }
    }
    pub fn add_torrent_using_file_path(&mut self, torrent_file_path: String) {
        let torrent_meta_file = match MetaInfo::new(&torrent_file_path) {
            Ok(mi) => mi,
            Err(e) => {
                panic!("{}", e);
            }
        };
        let transformed_torrent_meta =
            ClientTorrentMeta::from_torrent_file_meta(torrent_meta_file);
        self.torrents.push(transformed_torrent_meta);
    }
    #[allow(dead_code)]
    pub fn add_torrent_from_meta_info(&mut self, torrent_meta_file: MetaInfo) {
        let transformed_torrent_meta =
            ClientTorrentMeta::from_torrent_file_meta(torrent_meta_file);
        self.torrents.push(transformed_torrent_meta);
    }
}

/// Client file transformation, let's represent single and multiple files in standard way.
#[allow(dead_code)]
pub struct LocalFile {
    path: PathBuf,
    size: u64, 
}

/// Contains all the torrent info that is transformed for easier client interactions
pub struct ClientTorrentMeta {
    // All file sizes in this struct are in bytes, file sizes in LocalFile are in mb
    pub raw_torrent: MetaInfo,
    pub files: Vec<LocalFile>,
    pub downloaded: String,
    pub uploaded: String,
    pub info_hash: [u8; 20],
    pub left: String,
    pub peers: Option<Vec<Peer>>,
}

impl ClientTorrentMeta {
    pub fn from_torrent_file_meta(torrent_file_meta: MetaInfo) -> ClientTorrentMeta {
        let mut client_files: Vec<LocalFile> = Vec::new();
        let files = torrent_file_meta.files();
        for (_, content) in files.iter().enumerate() {
            let path_buf: PathBuf = content.path.iter().collect();
            client_files.push(LocalFile {
                path: path_buf,
                size: content.length / 1000000,
            });
        }

        // acquire hash digest in the form of GenericArray,
        // which in this case is equivalent to [u8; 20], sha-1 is alwyas 20 bytes.
        // Each u8 will give 2 hex letters, which need to be escaped using \x to represent hex.
        // ==> 40 hex chars
        // create a Sha1 object
        let mut hasher = Sha1::new();
        hasher.update(torrent_file_meta.info_bencoded_binary_form().unwrap());
        let info_hash = hasher.finalize();
        return ClientTorrentMeta {
            files: client_files,
            downloaded: 0.to_string(),
            uploaded: 0.to_string(),
            info_hash: info_hash.into(),
            left: torrent_file_meta.download_size().to_string(),
            raw_torrent: torrent_file_meta,
            peers : None,
        };
    }
}
