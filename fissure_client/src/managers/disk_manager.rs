
use std::path::PathBuf;

use tokio::sync::mpsc;
use log::error;
use tokio::fs::{File, OpenOptions, create_dir_all};

use crate::{bee_processor::bee_decoder::FissureErr, managers::{client_manager::LocalFile, piece_manager::PieceRequestManager}};

pub struct DiskManagerActor{
    recv: mpsc::Receiver<DiskManagerMessage>,
    files: Vec<LocalFile>,
    handles: Vec<File>
    //The ordering of the files is criticl as the torrent just
    //considers it as one big contigous stream of bites
}

impl DiskManagerActor{
    pub async fn new(files: Vec<LocalFile>, recv: mpsc::Receiver<DiskManagerMessage>, donwload_path: String) -> Result<Self, FissureErr>{
        let base_path = PathBuf::from(donwload_path);
        let mut handles = Vec::new();

        for file in files.iter(){
            let path = base_path.join(file.path.clone());
            if let Some(parent) = path.parent(){
                create_dir_all(parent).await.unwrap();
            }
            let handle = match OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(&path).await{
                Ok(h) => {
                    log::info!("Creating file as part of torrent init: {:?}", path);
                    h
                },
                Err(e) => {
                    error!("Error creating file during torrent init : {:?}", e);
                    return Err(FissureErr::new(e.to_string()));
                }
            };
            handles.push(handle);
        };
        Ok(DiskManagerActor{
            recv, 
            files, 
            handles
        })
    }
    async fn run(mut self) {
        while let Some(msg) = self.recv.recv().await {
            match msg {
                DiskManagerMessage::FlushPieceToDisk { piece_manager } => {
                    // write logic goes here -- see note below
                }
            }
        }
        log::info!("Disk manager channel closed, shutting down.");
    }
}

pub enum DiskManagerMessage{
    FlushPieceToDisk{
        piece_manager: PieceRequestManager,
    }
}

pub struct DiskManager{
    pub send: mpsc::Sender<DiskManagerMessage>,
    pub download_path: String
}

impl DiskManager{
    pub fn new(download_path: String, files: Vec<LocalFile>) -> Self{
        let(send, recv) = mpsc::channel(400);
        //TODO - create actor and spawn actor.run()
        let async_path = download_path.clone();
        tokio::spawn(async move {
            match DiskManagerActor::new(files, recv, async_path).await {
                Ok(actor) => actor.run().await,
                Err(e) => log::error!("Disk manager failed to initialize: {:?}", e),
            }
        });
        Self { send , download_path }
    }
}
