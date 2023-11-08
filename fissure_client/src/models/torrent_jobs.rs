use crate::orchestration::job_orchestrator::{Chunk, PieceProcess};

pub struct Job {
    index: u64,
    begin: u64,
    length: u64,
    /// Needs to be in bytes
    chunks: Vec<u8>, // We default to 16KB blocks!
}

impl Job {
    pub fn new(index: u64, length: u64, begin: u64) -> Self {
        Job {
            index,
            begin,
            length,
            chunks: vec![u8::from(0); length as usize],
        }
    }
    pub fn new_job_from_piece_process(pp: PieceProcess) -> Self {
        let length = match pp.chunk {
            Chunk::StandardChunk(_) => 16384,
            Chunk::PartialChunk(i, _) => i.into(),
        };
        Self::new(
            pp.index as u64,
            (pp.nth_chunk - 1) as u64 * 16384 as u64,
            length,
        )
    }
}
