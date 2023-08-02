/*!
    decodes the bencoded .torrent files and splits them into the properties
    defined in the initial bit torrent protocol (https://www.bittorrent.org/beps/bep_0003.html)

    // TODOs:
    - To move away from serde_bencoded to writing a parser from scratch
*/

use serde::{Deserialize, Serialize};
use serde_bencoded::from_bytes;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;

// If I'd have to match to a different name
// #[serde(rename = "piece length")]

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileInfo {
    length: u64,       // Size of each file
    path: Vec<String>, // Path of the file, not sure if relative or not. Should be parseable by PathBuf
}

#[derive(Serialize, Deserialize, Debug)]
struct Info {
    //In the single file case, the name key is the name of a file, in the muliple file case, it's the name of a directory.
    name: String, // The file name/path to store...only reccomended
    #[serde(rename = "piece length")]
    piece_length: u64, //Size of each piece the file is split into
    #[serde(rename = "pieces")]
    #[serde(with = "serde_bytes")]
    pieces_hash: Vec<u8>, // SHA-1 of all the piece stiched together, each sha-1 is 20 in length
    length: Option<u64>, // Exists only for single file downloads, tells length of file
    files: Option<Vec<FileInfo>>, // Exists only if multi file downloads
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MetaInfo {
    announce: String, // Contains the url for the tracker
    info: Info,
}

#[derive(Debug)]
pub struct BeeDecoderErr {
    error_string: String,
}

impl fmt::Display for BeeDecoderErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.error_string)
    }
}

impl Error for BeeDecoderErr {}

impl MetaInfo {
    pub fn new(torrent_file_path: &str) -> Result<MetaInfo, BeeDecoderErr> {
        let mut torrent_file = match File::open(torrent_file_path) {
            Ok(f) => f,
            Err(e) => {
                println!("Possible that the path doesn't exist, read the below logs to know more.");
                return Err(BeeDecoderErr {
                    error_string: e.to_string(),
                });
            }
        };
        let mut torrent_file_content: Vec<u8> = Vec::new();
        let file_size = match torrent_file.read_to_end(&mut torrent_file_content) {
            Ok(size) => size,
            Err(e) => {
                println!("Error reading file, possible not enough permissions, check below logs to know more.");
                return Err(BeeDecoderErr {
                    error_string: e.to_string(),
                });
            }
        };
        let meta_info: MetaInfo = match from_bytes(&torrent_file_content) {
            Ok(des) => des,
            Err(e) => {
                println!("Something went wrong deserializing torrent file content, maybe corrupted. Check below logs for more.");
                return Err(BeeDecoderErr {
                    error_string: e.to_string(),
                });
            }
        };
        println!(
            "Torrent file {} read. Size of file in characters : {}",
            torrent_file_path, file_size
        );
        //Debug print :
        // println!("{:#?}", meta_info);
        println!("Adding the following torrent to list : ");
        println!("name : {:?}", meta_info.info.name);
        println!("announce url : {:?}", meta_info.announce);
        let files = if meta_info.info.length.is_some() {
            vec![FileInfo {
                length: meta_info.info.length.unwrap(),
                path: vec![meta_info.info.name.clone()],
            }]
        } else {
            meta_info.info.files.clone().unwrap()
        };
        println!("file : {:?}", files);
        return Ok(meta_info);
    }
}
