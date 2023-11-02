struct Job {
    index: u64,
    length: u64,
    chunks: Vec<[u8]>, // We default to 16KB blocks!
}


impl Job{
    pub fn new(index : u64, length: u64) -> Self{

    }
}
