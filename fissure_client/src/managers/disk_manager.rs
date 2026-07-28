
use std::{path::PathBuf, sync::Arc};

use tokio::{io::{AsyncSeekExt, AsyncWriteExt}, sync::mpsc};
use log::error;
use tokio::fs::{File, OpenOptions, create_dir_all};

use crate::{bee_processor::bee_decoder::FissureErr, managers::{client_manager::LocalFile, piece_manager::PieceRequestManager}};

/**
 * In general, disk manager doesnt have to bother with blocks.
 * Yes, pieces can span 
 */

pub struct DiskManagerActor{
    recv: mpsc::Receiver<DiskManagerMessage>,
    std_piece_len : u64, //what len of each piece is in bytes except the last piece
    files: Vec<LocalFile>,
    handles: Vec<(u64, File)>
    //The ordering of the files is criticl as the torrent just
    //considers it as one big contigous stream of bites
}

impl DiskManagerActor{
    pub async fn new(files: Vec<LocalFile>, recv: mpsc::Receiver<DiskManagerMessage>, download_path: String, std_piece_len: u64) -> Result<Self, FissureErr>{
        let base_path = PathBuf::from(download_path);
        let mut byte_offset : u64 = 0;
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
                    log::debug!("Creating file as part of torrent init: {:?}", path);
                    h
                },
                Err(e) => {
                    error!("Error creating file during torrent init : {:?}", e);
                    return Err(FissureErr::new(e.to_string()));
                }
            };
            handles.push((byte_offset, handle));
            byte_offset +=  file.size;
        };
        Ok(DiskManagerActor{
            recv, 
            std_piece_len,
            files, 
            handles
        })
    }
    async fn run(mut self) {
        while let Some(msg) = self.recv.recv().await {
            match msg {
                DiskManagerMessage::FlushPieceToDisk { piece_manager } => {
                    let start_byte_offset = piece_manager.index * self.std_piece_len;
                    let end_byte_offset = start_byte_offset + piece_manager.piece_length;
                    let mut bytes_pending = end_byte_offset - start_byte_offset;
                    for (idx, (file_start_offset, file_handle)) in self.handles.iter_mut().enumerate(){
                        let file_end_offset = *file_start_offset + self.files.get(idx).unwrap().size;
                        if bytes_pending == 0{
                            break;
                        }
                        if start_byte_offset >= file_end_offset{
                            continue;
                        }
                        if end_byte_offset <= *file_start_offset{
                            break;
                        }
                        let write_offset = if start_byte_offset < *file_start_offset {
                            0
                        } else {
                            start_byte_offset - *file_start_offset
                        };
                        let bytes_remaining_in_file = file_end_offset - (*file_start_offset + write_offset);
                        let max_bytes_in_this_file = if bytes_pending > bytes_remaining_in_file {
                            bytes_remaining_in_file
                        } else {
                            bytes_pending
                        };
                        match file_handle.seek(std::io::SeekFrom::Start(write_offset)).await{
                            Ok(_) => {}, 
                            Err(e) => {
                                error!("Error seeking to posistion : {} in file: {:?} for piece: {}. Error: {}", write_offset, file_handle.metadata().await.unwrap(), piece_manager.index, e);
                            }
                        }
                        let piece_offset = piece_manager.piece_length-bytes_pending;
                        let data = piece_manager.get_data_with_offset(piece_offset, piece_offset + max_bytes_in_this_file).await;
                        match file_handle.write_all(&data).await{
                            Ok(_)=> {},
                            Err(e) => {
                                error!("Error writing to posistion : {} in file: {:?} for piece: {}. Error: {}", write_offset, file_handle.metadata().await.unwrap(), piece_manager.index, e);
                            }
                        }

                        bytes_pending -= max_bytes_in_this_file;

                    }
                    log::debug!("Persisted piece to disk successfully.");
                }
            }
        }
        log::info!("Disk manager channel closed, shutting down.");
    }
}

pub enum DiskManagerMessage{
    FlushPieceToDisk{
        piece_manager: Arc<PieceRequestManager>,
    }
}

#[derive(Clone, Debug)]
pub struct DiskManager{
    pub send: mpsc::Sender<DiskManagerMessage>,
    #[allow(dead_code)]
    pub download_path: String
}

impl DiskManager{
    pub fn new(download_path: String, files: Vec<LocalFile>, standard_piece_len: u64) -> Self{
        let(send, recv) = mpsc::channel(400);
        //TODO - create actor and spawn actor.run()
        let async_path = download_path.clone();
        tokio::spawn(async move {
            match DiskManagerActor::new(files, recv, async_path, standard_piece_len).await {
                Ok(actor) => actor.run().await,
                Err(e) => log::error!("Disk manager failed to initialize: {:?}", e),
            }
        });
        Self { send , download_path }
    }

    pub async fn flush_piece_to_disk(&self, piece_manager: Arc<PieceRequestManager>){
        let _ = self.send.send(DiskManagerMessage::FlushPieceToDisk { piece_manager: piece_manager.clone() }).await;

    }
}
