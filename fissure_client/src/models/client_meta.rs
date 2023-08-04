use super::torrent_meta::MetaInfo;
use std::path::PathBuf;
use crate::ClientEnv;
use crate::helper;

/*
  The clinet UI/Interface that needs transformations from
  torrent meta data is stores as ClientTorrentMeta
  The torrent functions still run using torrent_meta_info
*/


pub struct ClientState{
   pub peer_id : String, 
   pub torrents : Vec<ClientTorrentMeta>, //Modified transformed torrent meta
   pub torrents_raw : Vec<MetaInfo>,
   pub port : String
}

impl ClientState{
    pub fn new_client(client_env : ClientEnv) -> ClientState{
        //Peer id format : FS<4digit version><14 random char> : 20 chars long
        ClientState{
            peer_id : helper::generate_peer_id(&client_env),
            torrents : Vec::new(), 
            torrents_raw : Vec::new(), 
            port : client_env.port
        }
    }
    pub fn add_torrent_using_file_path(&mut self, torrent_file_path : String){
        let torrent_meta_file = match MetaInfo::new(&torrent_file_path){
            Ok(mi) => mi,
            Err(e) => {
                panic!("{}",e);
            }
        };
        let transformed_torrent_meta = ClientTorrentMeta::from_torrent_file_meta(&torrent_meta_file);
        self.torrents_raw.push(torrent_meta_file);
        self.torrents.push(transformed_torrent_meta);
    }
    pub fn add_torrent_from_meta_info(&mut self, torrent_meta_file : MetaInfo){
        let transformed_torrent_meta = ClientTorrentMeta::from_torrent_file_meta(&torrent_meta_file);
        self.torrents_raw.push(torrent_meta_file);
        self.torrents.push(transformed_torrent_meta);
    }
}

/// Client file transformation, let's represent single and multiple files in standard way.
pub struct LocalFile {
    path: PathBuf,
    size: u64,
}

/// Contains all the torrent info that is transformed for easier client interactions
pub struct ClientTorrentMeta {
    files: Vec<LocalFile>,
}

impl ClientTorrentMeta {
    pub fn from_torrent_file_meta(torrent_file_meta: &MetaInfo) -> ClientTorrentMeta {
        let mut client_files: Vec<LocalFile> = Vec::new();
        let files = torrent_file_meta.files();
        for (_, content) in files.iter().enumerate() {
            let path_buf: PathBuf = content.path.iter().collect();
            client_files.push(LocalFile {
                path: path_buf,
                size: content.length / 1000000,
            });
        }
        return ClientTorrentMeta {
            files: client_files,
        };
    }
}
