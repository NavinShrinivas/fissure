use std::{collections::HashMap, time::Instant};

use bitvec::prelude::*;
use sha1::{Digest, Sha1};
use tokio::sync::{mpsc, oneshot};

use crate::managers::piece_manager::PieceRequestManagerMessage::{BlockRecived, BlocksToRequest, GetData, GetDataWithLen};


#[derive(Debug)]
pub struct PieceRequestManagerActor{
    in_flight_timers: HashMap<u64, Instant>,
    last_piece_size : u64,
    downloaded_blocks: BitVec<u8>, //all the blocks we have recived so far
    blocks_in_flight: BitVec<u8>, //keeps track of blocks we have already sent but not recived.
    required_blocks: BitVec<u8>,
    data: Vec<u8>,
    hash: Vec<u8>,
    recv: mpsc::Receiver<PieceRequestManagerMessage>
}

const BLOCK_SIZE: u32 = 16_384;

impl PieceRequestManagerActor{
    pub fn new(p_len: u64, recv: mpsc::Receiver<PieceRequestManagerMessage>, hash: Vec<u8>) -> Self{

        let last_block_size = p_len % 16384;
        let no_of_blocks = if last_block_size == 0{
            p_len / 16384
        }else{
            p_len / 16384 + 1
        };

        let mut req_p = bitvec![u8, Lsb0; 0; no_of_blocks as usize];
        for i in 0..no_of_blocks {
            req_p.set(i as usize, true);
        };
        let mut data = Vec::new();
        data.resize(p_len as usize, 0u8);
        Self{
            in_flight_timers: HashMap::new(),
            last_piece_size : if last_block_size == 0 {16384} else {last_block_size},
            downloaded_blocks : bitvec![u8, Lsb0; 0; no_of_blocks as usize],
            blocks_in_flight: bitvec![u8, Lsb0; 0; no_of_blocks as usize],
            required_blocks: req_p, 
            data,
            recv,
            hash
        }
    }

    pub fn convert_block_offset_to_index(offset: usize) -> usize {
        (offset / BLOCK_SIZE as usize) as usize
    }
    pub fn convert_block_index_to_offset(index: usize) -> u64 { 
        (index as u64) * (BLOCK_SIZE as u64)
    }
    pub async fn run(mut self) {
        //TODO - do a smart lifetime based timeout for block request in flight

        //This ticker is the time a given peer can exclusively have the attempt at requesting a block
        //If not recieved within this time, this piecemanager is free to request them from other peer as well.
        //Although the blocks recived from previous peers are not discarded.
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(4));
        loop{
            tokio::select! {
                msg_opt = self.recv.recv() => {
                    match msg_opt {
                        Some(msg) => {
                            #[allow(unreachable_patterns)]
                            match msg{
                                //[TODO] => Add handlers for all other types of messages
                               BlocksToRequest { number_of_blocks, sender } => {
                                    log::debug!("PieceManager : Request for block request coming in");
                                    let mut requests_block_index = Vec::new();
                                    
                                    for idx in 0..self.required_blocks.len() {
                                        if requests_block_index.len() >= number_of_blocks as usize {
                                            break;
                                        }
                                        // Read the bitset state in-place directly
                                        if self.required_blocks[idx] && !self.blocks_in_flight[idx] && !self.downloaded_blocks[idx] {
                                            let block_size = if idx == self.required_blocks.len() - 1 { self.last_piece_size } else { 16384 };
                                            requests_block_index.push((Self::convert_block_index_to_offset(idx), block_size));
                                            self.blocks_in_flight.set(idx, true);
                                            self.in_flight_timers.insert(idx as u64, Instant::now());
                                        }
                                    }
                                    
                                    log::debug!("{:?}", requests_block_index);
                                    let _ = sender.send(requests_block_index); 
                                    log::debug!("PieceManager : Sent over a block req to peer");
                                },
                                BlockRecived { block_offset, data, piece_done_sender } => {
                                    log::debug!("PieceManager: recived block");
                                    let block_index = PieceRequestManagerActor::convert_block_offset_to_index(block_offset);
                                    
                                    self.in_flight_timers.remove(&(block_index as u64));
                                    self.required_blocks.set(block_index as usize, false);
                                    self.blocks_in_flight.set(block_index as usize, false); 
                                    self.downloaded_blocks.set(block_index as usize, true);
                                    
                                    let data_start_index = block_index as u64 * 16384;
                                    let end = data_start_index + data.len() as u64;
                                    
                                    // Ensure data buffer slicing safety
                                    if end as usize <= self.data.len() {
                                        self.data[data_start_index as usize..end as usize].copy_from_slice(&data);
                                    } else {
                                        log::error!("Incoming block write out of bounds! Index: {}, Size: {}", block_index, data.len());
                                    }

                                    if self.downloaded_blocks.all() {
                                        log::debug!("Checking hash of completed piece");
                                        let mut hasher = Sha1::new();
                                        hasher.update(&self.data);
                                        let result = hasher.finalize();
                                        
                                        if result.as_slice() == self.hash.as_slice() {
                                            let _ = piece_done_sender.send(true);
                                            log::debug!("Piece completely verified against hash record!");
                                        } else {
                                            log::warn!("All blocks of pieces received but hash doesn't match! Resetting piece.");
                                            
                                            self.downloaded_blocks.fill(false);
                                            self.required_blocks.fill(true);   
                                            self.blocks_in_flight.fill(false);
                                            self.in_flight_timers.clear();
                                        }
                                    }
                                    log::debug!("Recieved a block from peer!!");
                                }, 
                                GetData { start_offset, end_offset, sender } => {
                                    let start = start_offset as usize;
                                    let end = end_offset as usize;

                                    let slice = if end <= self.data.len() && start <= end {
                                        self.data[start..end].to_vec()
                                    } else {
                                        log::error!(
                                            "GetData out-of-bounds request: [{}, {}) but data len is {}",
                                            start, end, self.data.len()
                                        );
                                        Vec::new()
                                    };

                                    let _ = sender.send(slice);
                                },
                                GetDataWithLen { start_offset, len, sender } => {
                                    let start = start_offset as usize;
                                    let end = start + len as usize;

                                    let slice = if end <= self.data.len() && start <= end {
                                        self.data[start..end].to_vec()
                                    } else {
                                        log::error!(
                                            "GetDataWithLen out-of-bounds request: [{}, {}) but data len is {}",
                                            start, end, self.data.len()
                                        );
                                        Vec::new()
                                    };

                                    let _ = sender.send(slice);
                                },
                                _ => {
                                    log::error!("Unkown message recived in piece manager : {:?}", msg)
                                }
                            }
                        },
                        None => {
                            log::debug!("Piece manager for piece has been dropped");
                            break;
                        }
                    }
                },
                _ = ticker.tick() => {
                    let now = Instant::now();
                    let timeout = std::time::Duration::from_secs(5);

                    // Identify expired blocks
                    let expired: Vec<u64> = self.in_flight_timers.iter()
                        .filter(|(_, &time)| now.duration_since(time) > timeout)
                        .map(|(&idx, _)| idx)
                        .collect();

                    for idx in expired {
                        self.blocks_in_flight.set(idx as usize, false);
                        self.in_flight_timers.remove(&idx);
                        log::debug!("Block {} timed out", idx);
                    }
                }
            }
        }

    }
}

#[derive(Debug)]
pub enum PieceRequestManagerMessage{
    BlocksToRequest{
        number_of_blocks: u32,
        sender : oneshot::Sender<Vec<(u64, u64)>> //offset and len of the block
    },
    //TODO = Not sure if we want confirmation back from the fsm that the request was sent successfully
    //BEFORE we mark in the bitvec and the timout checker here
    BlockRecived{
        block_offset: usize,
        data: Vec<u8>,
        piece_done_sender: oneshot::Sender<bool>
        //to indicate if sha-1 piece was valid and all pieces are donwloaded
        //to the peer, the peer in turn will inform the 
        //manager to trigger the flush to disk
    },
    GetData {
        start_offset: u64,
        end_offset: u64,
        sender: oneshot::Sender<Vec<u8>>,
    },
    GetDataWithLen {
        start_offset: u64,
        len: u64,
        sender: oneshot::Sender<Vec<u8>>,
    },

}

#[derive(Debug)]
#[allow(dead_code)]
pub struct PieceRequestManager{
    //Only put things in here that wont be edited, if a particular data would be edited by peers, put it in actor
    pub index: u64,
    pub sha1_hash: Vec<u8>,
    pub piece_length : u64, //Not all pieces are of the same length
    send: mpsc::Sender<PieceRequestManagerMessage>
}

impl PieceRequestManager{
    pub fn new(index: u64, sha1_hash: Vec<u8>, p_len: u64) -> Self{
        let (send, recv) = mpsc::channel(400);
        let actor = PieceRequestManagerActor::new(p_len, recv, sha1_hash.clone());
        tokio::spawn(async move{actor.run().await});
        Self{
            index, 
            sha1_hash,
            piece_length: p_len,
            send
        }
    }
    pub async fn block_recvied(&self, block_offset: usize, data: Vec<u8> ) -> bool{
        let (tx,rx) = oneshot::channel::<bool>();
        let _ = self.send.send(PieceRequestManagerMessage::BlockRecived { block_offset, data, piece_done_sender: tx}).await;
        match rx.await.ok(){
            Some(v) => {v}
            None => false
        }
    }

    pub async fn request_block(&self, number_of_blocks: u32) -> Vec<(u64, u64)>{
        let (tx,rx) = oneshot::channel::<Vec<(u64, u64)>>();
        let _ = self.send.send(PieceRequestManagerMessage::BlocksToRequest { number_of_blocks, sender: tx }).await;
        match rx.await.ok(){
            Some(v) => {v}
            None => {
                log::debug!("Empty block req");
                Vec::new()
            }
        }
    }

    pub async fn get_data_with_offset(&self, start_offset: u64, end_offset: u64) -> Vec<u8>{
        let (tx, rx) = oneshot::channel::<Vec<u8>>();
        let _ = self.send.send(PieceRequestManagerMessage::GetData {
            start_offset,
            end_offset,
            sender: tx,
        }).await;

        rx.await.unwrap_or_default()
    }

    pub async fn get_data_with_len(&self, start_offset: u64, len: u64) -> Vec<u8>{
        let (tx, rx) = oneshot::channel::<Vec<u8>>();
        let _ = self.send.send(PieceRequestManagerMessage::GetDataWithLen { start_offset, len, sender: tx }).await;
        rx.await.unwrap_or_default()
    }
}


