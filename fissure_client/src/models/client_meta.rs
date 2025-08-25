use super::torrent_meta::MetaInfo;
use crate::helper;
use crate::models::torrent_jobs;
use crate::models::torrent_meta::Peer;
use crate::models::torrent_meta::TrackerResponse;
use crate::orchestration::{handshake_orechestration, job_orchestrator, torrent_refresh};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

//Represents the Client state from yaml settings file
#[derive(Serialize, Deserialize, Debug)]
pub struct FissureClientOptions {
    pub port: String,
    pub ip: String,
    pub version: String,
}

impl Default for FissureClientOptions {
    fn default() -> Self {
        FissureClientOptions {
            port: "6001".to_string(),
            ip: "0.0.0.0".to_string(),
            version: "0001".to_string(),
        }
    }
}

/*
  The client UI/Interface that needs transformations from
  torrent meta data is stored as ClientTorrentMetaInfo
  The torrent functions still run using torrent_meta_info
*/

pub struct Client {
    pub peer_id: String,
    pub torrents: HashMap<String, Arc<RwLock<ClientTorrentMetaInfo>>>, //Modified transformed torrent meta, used for ease
    pub port: String,
}

impl Client {
    pub fn new_client(
        settings: &std::collections::HashMap<String, serde_yaml::value::Value>,
    ) -> Client {
        //Peer id format : FS<4digit version><14 random char> : 20 chars long

        let client_options = crate::settingYaml::settingYaml::get_inner_value(
            settings,
            vec!["client".to_string()],
            FissureClientOptions::default(),
        );

        Client {
            peer_id: helper::generate_peer_id(&client_options),
            torrents: HashMap::new(),
            port: client_options.port,
        }
    }
    pub fn add_torrent_using_file_path(&mut self, torrent_file_path: String) -> String {
        let torrent_meta_file = match MetaInfo::new(&torrent_file_path) {
            Ok(mi) => mi,
            Err(e) => {
                panic!("[ERROR] {}", e);
            }
        };
        let transformed_torrent_meta =
            ClientTorrentMetaInfo::from_torrent_file_meta(torrent_meta_file);
        let name = transformed_torrent_meta.get_torrent_name();
        self.torrents.insert(
            transformed_torrent_meta.get_torrent_name(),
            Arc::new(RwLock::new(transformed_torrent_meta)),
        );
        return name;
    }
    //Unused :
    // #[allow(dead_code)]
    // pub fn add_torrent_from_meta_info(&mut self, torrent_meta_file: MetaInfo) {
    //     // let transformed_torrent_meta =
    //         // ClientTorrentMetaInfo::from_torrent_file_meta(torrent_meta_file);
    //     // self.torrents.push(&transformed_torrent_meta);
    // }
    pub async fn orchestrate_download(&self, arc_mutex_ctmi: Arc<RwLock<ClientTorrentMetaInfo>>) {
        let arc_mutex_ctmi_inner = arc_mutex_ctmi.clone();
        let arc_mutex_ctmi_inner2 = arc_mutex_ctmi.clone();

        //Send peers from tracker to handshake :
        let (peer_tracker_handshake_channel_tx, peer_tracker_handshake_channel_rx) =
            crossbeam_channel::bounded::<TrackerResponse>(300);

        //Send unfinished jobs/pending jobs to peer state machine
        let (unfinished_job_snd, unfinished_job_recv) =
            crossbeam_channel::bounded::<torrent_jobs::Job>(2000);

        //send finished (downloaded pieces) from peer state machines to file assembler
        let (finished_job_snd, finished_job_recv) =
            crossbeam_channel::unbounded::<torrent_jobs::Job>();

        let (unfinished_job_snd_handshake, unfinished_job_recv_handshake) =
            (unfinished_job_snd.clone(), unfinished_job_recv.clone());

        let unfinished_job_snd_job_orchestrator = unfinished_job_snd.clone();

        let peer_id1 = self.peer_id.clone();
        let peer_id2 = self.peer_id.clone();
        let port = self.port.clone();

        tokio::spawn(async move {
            torrent_refresh::torrent_refresh(
                arc_mutex_ctmi_inner,
                &peer_id1,
                &port,
                peer_tracker_handshake_channel_tx,
            )
            .await
        });

        let piece_mem_rep = Arc::new(job_orchestrator::file_piece_memory_representation(
            &arc_mutex_ctmi.read().await.raw_torrent,
        ));

        let piece_mem_rep_state_machine_clone = piece_mem_rep.clone();

        //too much contention in piece_mem_rep for job scheduler and piece assembler

        tokio::spawn(async move {
            //handshake_orchestrator first initiates a connection to each peer from peer_tracker channel
            //and spawns a peer_protocol state machine for each connection, where each state machine
            //needs a unfinished_job send and recv
            //[TODO] It will also need finished_job send to send the pieces to file assembler
            handshake_orechestration::handshake_orchestrator(
                peer_tracker_handshake_channel_rx,
                arc_mutex_ctmi_inner2,
                &peer_id2,
                unfinished_job_snd_handshake,  //Given to state machine
                unfinished_job_recv_handshake, //Given to state machine
                piece_mem_rep_state_machine_clone, //passed to state machine for upload
            )
            .await
        });


        tokio::spawn(async move {
            job_orchestrator::job_orchestrator(
                piece_mem_rep,
                unfinished_job_snd_job_orchestrator,
            )
            .await
        });

        //TODO : Piece assembler thread
        //TODO : File assembler thread
        //TODO : Tracker announcer thread

        loop {
            // Busy wait
        }
    }
}

/// Client file transformation, let's represent single and multiple files in standard way.
#[allow(dead_code)]
#[derive(Clone)]
pub struct LocalFile {
    path: PathBuf,
    size: u64,
}

/// Contains all the torrent info that is transformed for easier client interactions
#[derive(Clone)]
pub struct ClientTorrentMetaInfo {
    // All file sizes in this struct are in bytes, file sizes in LocalFile are in mb
    pub raw_torrent: MetaInfo,
    pub files: Vec<LocalFile>,
    pub downloaded: String,
    pub uploaded: String,
    pub info_hash: [u8; 20],
    pub left: String,
    pub peers: Option<Vec<Peer>>,
    pub tracker_response: Option<TrackerResponse>,
}

impl ClientTorrentMetaInfo {
    #[allow(unused)]
    pub fn add_peer_list_from_tracker_response(&mut self, tracker_response: TrackerResponse) {
        self.peers = tracker_response.peers;
    }
    pub fn get_torrent_name(&self) -> String {
        return self.raw_torrent.info.name.clone();
    }
    pub fn from_torrent_file_meta(torrent_file_meta: MetaInfo) -> ClientTorrentMetaInfo {
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
        // which in this case is equivalent to [u8; 20], sha-1 is always 20 bytes.
        // Each u8 will give 2 hex letters, which need to be escaped using \x to represent hex.
        // ==> 40 hex chars
        // create a Sha1 object
        let mut hasher = Sha1::new();
        hasher.update(torrent_file_meta.info_bencoded_binary_form().unwrap());
        let info_hash = hasher.finalize();
        return ClientTorrentMetaInfo {
            files: client_files,
            downloaded: 0.to_string(),
            uploaded: 0.to_string(),
            info_hash: info_hash.into(),
            left: torrent_file_meta.download_size().to_string(),
            raw_torrent: torrent_file_meta,
            peers: None,
            tracker_response: None,
        };
    }
}
