use crate::models::torrent_meta::MetaInfo;
use crate::models::torrent_jobs;
use crossbeam_channel;
use rand::Rng;


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
    StandardChunk(Vec<u8>),
    PartialChunk(u32, Vec<u8>),
}

#[derive(Clone)]
pub struct PieceProcess {
    pub index: usize,
    pub nth_chunk: usize, //1-indexed chunk pointer
    pub chunk: Chunk,
}

impl PieceProcess {
    pub fn new(index: usize, number_of_chunks: usize) -> Vec<Self> {
        let mut temp: Vec<Self> = Vec::new();
        for i in 0..number_of_chunks {
            temp.push(Self {
                index,
                nth_chunk: i + 1,
                chunk: Chunk::StandardChunk(Vec::new()),
            });
        }
        return temp;
    }
    pub fn new_non_full_pieces(
        index: usize,
        number_of_full_chunks: usize,
        size_of_partial_chunk: u32,
    ) -> Vec<Self> {
        let mut temp: Vec<Self> = Vec::new();
        for i in 0..number_of_full_chunks {
            temp.push(Self {
                index,
                nth_chunk: i + 1,
                chunk: Chunk::StandardChunk(Vec::new()),
            });
        }

        temp.push(PieceProcess {
            index,
            nth_chunk: number_of_full_chunks + 1,
            chunk: Chunk::PartialChunk(size_of_partial_chunk, Vec::new()),
        });
        return temp;
    }
    pub fn torrent_piece_state(size: usize, piece_size: usize) -> Vec<Self> {
        let upper_index = size / piece_size; // Both are in bytes (from MetaInfo)
        let full_pieces = upper_index - 1; //If there is a partial piece
        let mut temp_piece_state: Vec<Self> = Vec::new();
        for i in 0..full_pieces {
            temp_piece_state.extend(Self::new(i, piece_size / 16384));
        }
        let number_of_full_chunks_in_last_piece =
            (((size - (full_pieces * piece_size)) / 16384) as f64).floor();
        let size_of_non_full_chunk = (size - (full_pieces * piece_size)) as u32
            - (number_of_full_chunks_in_last_piece as u32 * 16384) as u32; // Only 1 non full chunk possible
        temp_piece_state.extend(Self::new_non_full_pieces(
            full_pieces,
            number_of_full_chunks_in_last_piece as usize,
            size_of_non_full_chunk,
        ));
        return temp_piece_state;
    }
}

pub async fn job_orchestrator(
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
    torrent_meta_info: &MetaInfo, //Non Client torrent meta info
) {
    // Needs to determines chunks from pieces and send it across channel
    // Processing to create a "state" of all possible chunks
    println!("[DEBUG] starting job orchestration");
    let raw_torrent = &torrent_meta_info.info;
    let mut piece_state = PieceProcess::torrent_piece_state(
        raw_torrent.length.unwrap() as usize,
        raw_torrent.piece_length as usize,
    );

    // Test (To see is number of chunks and len of representation is same) :
    let mut tot_len = 0;
    let mut chunks = 0;
    for i in piece_state.iter() {
        chunks += 1;
        match i.chunk {
            Chunk::StandardChunk(_) => {
                tot_len += 16384;
            }
            Chunk::PartialChunk(chunk_size, _) => {
                tot_len += chunk_size;
            }
        }
    }
    println!("tot size : {}, chunks : {}", tot_len, chunks);
    loop {
        if piece_state.len() == 0 {
            break;
        }
        if unfinished_job_snd.len() > 20 {
            // We dont buffer more than 20 pieces, we might want to write some logic to see if
            // there is no progress in the chunks in the channel, in which case we'd have to move
            // on to other chunks
            continue;
        } else {
            let mut rng = rand::thread_rng();
            let chunk_work = piece_state.remove(rng.gen_range(0..piece_state.len()));
            let job = torrent_jobs::Job::new_job_from_piece_process(chunk_work);
            let s = unfinished_job_snd.clone();
            s.send(job).unwrap(); // awaits till read happens on the other side, I dont like
                                  // it...but thats how "unbounded" channels work in crossbeam,
                                  // maybe I should use bounded hmnmnmn
        }
    }
}
