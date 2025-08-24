use crate::orchestration::job_orchestrator::{Chunk, MemoryPiece};

//Job are chuk level granular
pub struct Job {
    pub index: u64,
    pub begin: u64,
    pub length: u64,
    /// Needs to be in bytes
    pub chunk_data: Vec<u8>, // We default to 16KB blocks!
}

impl Job {
    pub fn new(index: u64, length: u64, begin: u64) -> Self {
        Job {
            index,
            begin,
            length,
            chunk_data: vec![u8::from(0); length as usize],
        }
    }
    pub fn new_job_from_piece_process(piece_index: u32, pp: Chunk) -> Self {
        let (length, index) = match pp {
            Chunk::StandardChunk(index, _) => (16384, index),
            Chunk::PartialChunk(index, i, _) => (i.into(), index),
        };
        Self::new(
            piece_index as u64, //index
            length,             //length of chunk
            //offset within piece (i.e start pos of this chunk)
            (index - 1) as u64 * 16384 as u64,
        )
    }
}
