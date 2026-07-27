use crate::managers::torrent_manager::TorrentManager;
use crate::models::peer_settings::PeerOptions;
use crate::models::torrent_meta::MetaInfo;
use crate::{helper, settingYaml};

use crate::orchestration::{handshake_orechestration, torrent_refresh};
use crate::protocols::tracker::TrackerResponse;
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use sha1::{Digest, Sha1};
use tokio::sync::mpsc;

use std::collections::HashMap;
use std::path::PathBuf;


//Represents the Client state from yaml settings file
#[derive(Serialize, Deserialize, Debug)]
pub struct FissureClientOptions {
    pub port: String,
    pub ip: String,
    pub version: String,
}
//Default values for client if not provided in the setting.yaml file:
impl Default for FissureClientOptions {
    fn default() -> Self {
        FissureClientOptions {
            port: "6001".to_string(),
            ip: "0.0.0.0".to_string(),
            version: "0001".to_string(),
        }
    }
}
#[derive(Debug)]
pub enum ClientActorMessage{
    AddTorret{
        torrent_file_path: String,
        peer_settings: PeerOptions,
        download_path: String
    }
}

pub struct ClientActor {
    pub peer_id: String, //our id
    pub torrent_managers: Vec<TorrentManager>, //managers that actually manage each torrent download
    pub port: String, //port in which our client will listen in
    pub recv : mpsc::Receiver<ClientActorMessage>
}

impl ClientActor {
    pub fn new(
        settings: &std::collections::HashMap<String, serde_yaml::value::Value>,
        recv: mpsc::Receiver<ClientActorMessage>
    ) -> Self {
        //Peer id format : FS<4digit version><14 random char> : 20 chars long

        let client_options = crate::settingYaml::settingYaml::get_inner_value(
            settings,
            vec!["client".to_string()],
            FissureClientOptions::default(),
        );

        Self {
            peer_id: helper::generate_peer_id(&client_options),
            torrent_managers: Vec::new(),
            port: client_options.port,
            recv
        }
    }
    async fn run(mut self) {
        while let Some(msg) = self.recv.recv().await{
            match msg{
                ClientActorMessage::AddTorret { torrent_file_path, peer_settings: _, download_path } => {
                    //First create manager 
                    let ctmi = ClientTorrentMetaInfo::from_torrent_file_path(torrent_file_path);
                    let manager : TorrentManager = TorrentManager::new(ctmi, download_path);
                    self.torrent_managers.push(manager.clone());
                    let (peer_tracker_tx, peer_tracker_rx) = tokio::sync::mpsc::channel::<TrackerResponse>(300);
                    //spawn tracker refresh
                    let c1 = manager.clone();
                    let c1_peer = self.peer_id.clone();
                    let c1_port = self.port.clone();
                    let c2_peer = self.peer_id.clone();
                    let h1 = tokio::spawn(async move{
                        torrent_refresh::torrent_refresh(c1, c1_peer, c1_port, peer_tracker_tx).await;
                    });
                    //spawn handshaker and pass it a torrentmanager 
                    //This handshake orechestration also spawn the FSM and provides it with all the needed details.
                    let h2 = tokio::spawn(async move{
                        handshake_orechestration::handshake_orchestrator(peer_tracker_rx, manager.clone(), c2_peer).await;
                    });
                    let _ = join_all(vec![h1, h2]).await;

                },
                _ => {
                    log::error!("Recived wrong message in Client actor: {:?}", msg)
                }
            }
        }

    }
}

pub struct ClientManager{
    settings: FissureClientOptions,
    raw_settings: HashMap<String, Value>,
    send: mpsc::Sender<ClientActorMessage>,

}
impl ClientManager{
    pub fn new(settings: &HashMap<std::string::String, serde_yaml::Value>) -> Self{
        let (send, recv) = mpsc::channel(400);
        log::info!("Creating new client maanger");
        let actor = ClientActor::new(&settings, recv);
        tokio::spawn(async move{actor.run().await});
        Self{
            settings: settingYaml::settingYaml::get_inner_value(settings, vec!["client".to_string()], FissureClientOptions::default()),
            raw_settings: settings.clone(),
            send
        }

    }
    pub async fn add_torrent(&self, torrent_file_path: String, download_path: String){
        let peer_settings = settingYaml::settingYaml::get_inner_value(
            &self.raw_settings, 
            vec!["client".to_string(), "peer_settings".to_string()], 
            PeerOptions::default());
        let _ = self.send.send(ClientActorMessage::AddTorret { torrent_file_path, peer_settings, download_path }).await;
    }
}

/// Client file transformation, let's represent single and multiple files in standard way.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct LocalFile {
    pub path: PathBuf,
    pub size: u64, //in megabytes
}

/**
 * Contains all the state the client needs to be able
 * to make the donwload and upload happen
 * is ideally immutable and never changes, so
 * wrapping in Arc is often enough
 */
#[derive(Clone, Debug)]
pub struct ClientTorrentMetaInfo {
    // All file sizes in this struct are in bytes, file sizes in LocalFile are in mb
    pub raw_torrent: MetaInfo,
    pub files: Vec<LocalFile>,
    //The ordering of the files is criticl as the torrent just
    //considers it as one big contigous stream of bites
    pub info_hash: [u8; 20], //hash fo the torrent info itself, not the pieces
}

impl ClientTorrentMetaInfo {
    pub fn from_torrent_file_meta(torrent_file_meta: MetaInfo) -> ClientTorrentMetaInfo {
        let mut client_files: Vec<LocalFile> = Vec::new();
        let files = torrent_file_meta.files();
        for (_, content) in files.iter().enumerate() {
            let path_buf: PathBuf = content.path.iter().collect();
            client_files.push(LocalFile {
                path: path_buf,
                size: content.length / 1000000, //in megabytes
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
            info_hash: info_hash.into(),
            raw_torrent: torrent_file_meta,
        };
    }

    pub fn from_torrent_file_path(file_path : String) -> Self{
        let torrent_meta_file = match MetaInfo::new(&file_path) {
            Ok(mi) => mi,
            Err(e) => {
                panic!("[ERROR] {}", e);
            }
        };
        let transformed_torrent_meta =
            ClientTorrentMetaInfo::from_torrent_file_meta(torrent_meta_file);
        transformed_torrent_meta
    }
}
