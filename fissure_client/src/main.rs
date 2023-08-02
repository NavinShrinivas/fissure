mod bee_processor;
use bee_processor::bee_decoder;

fn main() {
    println!("Hello, world. Starting fissure client!");
    let torrent_meta_info = match bee_decoder::MetaInfo::new("../test_torrent_files/test.torrent"){
        Ok(meta) => meta,
        Err(e) =>{
            panic!("{:?}",e);
        }
    };
}
