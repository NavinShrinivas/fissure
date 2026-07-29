
use std::{path::PathBuf, sync::Arc};

use tokio::{io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt}, sync::{mpsc, oneshot}};
use log::error;
use tokio::fs::{File, OpenOptions, create_dir_all};

use crate::{bee_processor::bee_decoder::FissureErr, managers::{client_manager::LocalFile, disk_manager::DiskManagerMessage::GetDataWithLength, piece_manager::PieceRequestManager}};

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
                DiskManagerMessage::FlushPieceToDisk { piece_manager  , send } => {
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
                                let _ = send.send(false);
                                return;
                            }
                        }
                        let piece_offset = piece_manager.piece_length-bytes_pending;
                        let data = piece_manager.get_data_with_offset(piece_offset, piece_offset + max_bytes_in_this_file).await;
                        match file_handle.write_all(&data).await{
                            Ok(_)=> {},
                            Err(e) => {
                                error!("Error writing to posistion : {} in file: {:?} for piece: {}. Error: {}", write_offset, file_handle.metadata().await.unwrap(), piece_manager.index, e);
                                let _ = send.send(false);
                                return;
                            }
                        }

                        bytes_pending -= max_bytes_in_this_file;

                    }
                    let _ = send.send(true);
                    log::debug!("Persisted piece to disk successfully.");
                },
                GetDataWithLength { piece_index, offset, length, reply } => {
                    let start_byte_offset = piece_index as u64 * self.std_piece_len + offset as u64;
                    let end_byte_offset = start_byte_offset + length as u64;
                    let mut bytes_pending = end_byte_offset - start_byte_offset;
                    let mut data = Vec::new();
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
                        let read_offset = if start_byte_offset < *file_start_offset {
                            //reading from start of file
                            0
                        } else {
                            //reading from between
                            start_byte_offset - *file_start_offset
                        };
                        let bytes_remaining_in_file = file_end_offset - (*file_start_offset + read_offset);
                        let max_bytes_we_can_read_from_this_file = if bytes_pending > bytes_remaining_in_file {
                            bytes_remaining_in_file
                        } else {
                            bytes_pending
                        };
                        match file_handle.seek(std::io::SeekFrom::Start(read_offset)).await{
                            Ok(_) => {}, 
                            Err(e) => {
                                error!("Error seeking to posistion : {} in file: {:?} for piece: {}. Error: {}", read_offset, file_handle.metadata().await.unwrap(), piece_index, e);
                                let _ = reply.send(None);
                                return;
                            }
                        }
                        let mut buffer = vec![0u8; max_bytes_we_can_read_from_this_file as usize];
                        match file_handle.read_exact(&mut buffer).await{
                            Ok(_)=> {
                                data.extend(buffer);
                            },
                            Err(e) => {
                                error!("Error reading from posistion : {} in file: {:?} for piece: {}. Error: {}", read_offset, file_handle.metadata().await.unwrap(), piece_index, e);
                                let _ = reply.send(None);
                                return;
                            }
                        }

                        bytes_pending -= max_bytes_we_can_read_from_this_file;

                    }
                    let _ = reply.send(Some(data));
                }
            }
        }
        log::info!("Disk manager channel closed, shutting down.");
    }
}

pub enum DiskManagerMessage{
    FlushPieceToDisk{
        piece_manager: Arc<PieceRequestManager>,
        send: oneshot::Sender<bool>
    },
    GetDataWithLength{
        piece_index: u32,
        offset: u32,
        length: u32,
        reply: tokio::sync::oneshot::Sender<Option<Vec<u8>>>
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

    pub async fn flush_piece_to_disk(&self, piece_manager: Arc<PieceRequestManager>) -> bool{
        let (send, recv) = oneshot::channel();
        let _ = self.send.send(DiskManagerMessage::FlushPieceToDisk { piece_manager: piece_manager.clone() , send}).await;
        match recv.await{
            Ok(res) => res,
            Err(e) => {
                log::error!("Error reciving flush piece to disk result from disk manager for piece: {}. Error: {}", piece_manager.index, e);
                false
            }
        }

    }

    pub async fn get_data_with_length(&self, piece_index: u32, offset: u32, length: u32) -> Option<Vec<u8>>{
        let (reply_send, reply_recv) = tokio::sync::oneshot::channel();
        let _ = self.send.send(DiskManagerMessage::GetDataWithLength { piece_index, offset, length, reply: reply_send }).await;
        match reply_recv.await{
            Ok(data) => data,
            Err(e) => {
                log::error!("Error reciving data from disk manager for piece: {}, offset: {}, length: {}. Error: {}", piece_index, offset, length, e);
                None
            }
        }
    }
}
