use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;

use crate::models::client_meta::ClientTorrentMetaInfo;
use crate::models::torrent_jobs;
use crate::{models::torrent_meta::MetaInfo, orchestration::job_orchestrator};
use byte_unit::Byte;
use crossbeam_channel;
use dashmap::DashMap;
use log::{debug, error, info};
use rand::Rng;
use tokio::sync::RwLock;

/* Hirerarchy files in torrents :
 * A file is made up of pieces, each piece can be made up of chunks where each chunk can maximum be 2^14 bytes (16384 bytes)
 * Size of each piece can vary and is mentioned in the meta info the bencoded torrent file
 * PieceProcess in this file, is an in memory representation of individual chunks of the file
 * where, we can locate its posistion in the file, by knowing the index of the piece, and index
 * (this is 1-indexed) of the chunk within the piece
 *
 * Requests for these chunks, instead of using index for offset within the peice, they use byte
 * offset within the piece
 *
 */

#[derive(Clone)]
pub enum Chunk {
    StandardChunk(u32, Vec<u8>),     //index, data
    PartialChunk(u32, u32, Vec<u8>), //index, size, data
}

//PieceProcess is a piece level granular memory rep of data
#[derive(Clone)]
pub struct MemoryPiece {
    pub index: usize,            //index of piece
    pub chunks: Vec<Chunk>,      //all the chunks within the piece
    pub integrity_hash: Vec<u8>, //sha1 hash of piece to verify integrity
}

impl MemoryPiece {
    pub fn new(index: usize, number_of_chunks: usize, hash: Vec<u8>) -> Self {
        let mut chunks: Vec<Chunk> = Vec::new();
        for i in 0..number_of_chunks {
            chunks.push(Chunk::StandardChunk(i as u32 + 1, Vec::new()));
        }
        Self {
            index,
            chunks,
            integrity_hash: hash,
        }
    }
    pub fn new_non_full_pieces(
        index: usize,
        number_of_full_chunks: usize,
        size_of_partial_chunk: u32,
        hash: Vec<u8>,
    ) -> Self {
        let mut chunks: Vec<Chunk> = Vec::new();
        for i in 0..number_of_full_chunks {
            chunks.push(Chunk::StandardChunk(i as u32 + 1, Vec::new()));
        }
        chunks.push(Chunk::PartialChunk(
            number_of_full_chunks as u32 + 2,
            size_of_partial_chunk,
            Vec::new(),
        ));

        Self {
            index,
            chunks,
            integrity_hash: hash,
        }
    }
    pub fn torrent_piece_state(
        size: usize,
        piece_size: usize,
        torrent_meta_info: &MetaInfo,
    ) -> Vec<Self> {
        let upper_index = size / piece_size; // Both are in bytes (from MetaInfo)
        let full_pieces = upper_index - 1; //If there is a partial piece
        let mut temp_piece_state: Vec<Self> = Vec::new();
        for i in 0..full_pieces {
            let piece_hash_start = i * 20;
            let piece_hash_end = piece_hash_start + 20;
            let piece_hash =
                torrent_meta_info.info.pieces_hash[piece_hash_start..piece_hash_end].to_vec();
            temp_piece_state.push(Self::new(i, piece_size / 16384, piece_hash));
        }
        let number_of_full_chunks_in_last_piece =
            (((size - (full_pieces * piece_size)) / 16384) as f64).floor();
        let size_of_non_full_chunk = (size - (full_pieces * piece_size)) as u32
            - (number_of_full_chunks_in_last_piece as u32 * 16384) as u32; // Only 1 non full chunk possible

        let last_piece_hash_start = full_pieces * 20;
        let last_piece_hash_end = last_piece_hash_start + 20;
        let last_piece_hash =
            torrent_meta_info.info.pieces_hash[last_piece_hash_start..last_piece_hash_end].to_vec();

        temp_piece_state.push(Self::new_non_full_pieces(
            full_pieces,
            number_of_full_chunks_in_last_piece as usize,
            size_of_non_full_chunk,
            last_piece_hash,
        ));
        return temp_piece_state;
    }
}

pub fn file_piece_memory_representation(
    torrent_meta_info: &MetaInfo, //Non Client torrent meta info
) -> DashMap<usize, MemoryPiece> {
    // Needs to determines chunks from pieces and send it across channel
    // Processing to create a "state" of all possible chunks
    let raw_torrent = &torrent_meta_info.info;
    let mut piece_state = MemoryPiece::torrent_piece_state(
        raw_torrent.length.unwrap() as usize,
        raw_torrent.piece_length as usize,
        torrent_meta_info,
    );

    // Test (To see is number of chunks and len of representation is same) :
    let mut tot_len = 0;
    let mut chunks = 0;
    for i in piece_state.iter() {
        for c in i.chunks.iter() {
            chunks += 1;
            match c {
                Chunk::StandardChunk(_, _) => {
                    tot_len += 16384;
                }
                Chunk::PartialChunk(_, chunk_size, _) => {
                    tot_len += chunk_size;
                }
            }
        }
    }
    let piece_state_map = DashMap::new();

    loop{
        let p = piece_state.pop();
        match p {
            Some(mem_piece) => {
                piece_state_map.insert(mem_piece.index, mem_piece);
            }
            None => break,
        }
    }

    let bytes = byte_unit::Byte::from_u64(tot_len as u64);
    let human_readable = bytes
        .get_appropriate_unit(byte_unit::UnitType::Binary)
        .to_string();
    info!(
        "tot accumulated chunks size : {}, chunks : {}",
        human_readable, chunks
    );

    return piece_state_map;
}
pub async fn job_orchestrator(
    piece_state_map: Arc<DashMap<usize, MemoryPiece>>,
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
) {
    let mut schedule_pieces: HashSet<u32> = HashSet::new();

    let total_pieces = piece_state_map.len();
    loop {
        if unfinished_job_snd.len() > 60 {
            // We dont buffer more than 60 pieces, [TODO] we might want to write some logic to see if
            // there is no progress in the chunks in the channel, in which case we'd have to move
            // on to other chunks

            //These 60 piece can be part of different pieces. If we have 60 dead chunks, this logic will get stuck
            continue;
        } else {

            //=====reading contentioned space=========
            let mut rng = rand::thread_rng();
            let mut random_index = rng.gen_range(0..total_pieces);
            while schedule_pieces.contains(&(random_index as u32)) {
                random_index = rng.gen_range(0..total_pieces);
            }
            let chunks_to_be_scheduled = match piece_state_map.get(&random_index){
                Some(v) => v.chunks.clone(),
                None => {
                    error!("Error reading piece state map for index : {}", random_index);
                    continue;
                }
            };
            //===========done reading================

            schedule_pieces.insert(random_index as u32);
            for c in chunks_to_be_scheduled.into_iter() {
                let job =
                    torrent_jobs::Job::new_job_from_piece_process(random_index as u32, c);
                let s = unfinished_job_snd.clone();
                debug!(
                    "Scheduling piece index : {} and chunk offset : {} out of total pieces : {}",
                    random_index, job.begin, total_pieces
                );
                s.send(job).unwrap(); // awaits till read happens on the other side, I dont like
                                      // it...but thats how "unbounded" channels work in crossbeam,
                                      // maybe I should use bounded hmnmnmn
            }
            if schedule_pieces.len() >= total_pieces {
                break;
            }
        }
    }
}
