/*!
    decodes the bencoded .torrent files and splits them into the properties
    defined in the initial bit torrent protocol (https://www.bittorrent.org/beps/bep_0003.html)

    // TODOs:
    - To move away from serde_bencoded to writing a parser from scratch
*/

use serde_bencoded::from_bytes;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use crate::models::torrent_meta::{MetaInfo, FileInfo};
use std::fmt;
use std::error::Error;

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
    pub fn files(&self) -> Vec<FileInfo> {
        let files = if self.info.length.is_some() {
            vec![FileInfo {
                length: self.info.length.unwrap(),
                path: vec![self.info.name.clone()],
            }]
        } else {
            self.info.files.clone().unwrap()
        };
        return files;
    }
    pub fn print_files(&self) {
        let files = self.files();
        for (index, content) in files.iter().enumerate() {
            let path_buf: PathBuf = content.path.iter().collect();
            println!(
                "\t\t {}. path : {:?}, size :{} MB",
                index + 1,
                path_buf,
                content.length / 1000000
            );
        }
    }
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
        println!("\t name : {:?}", meta_info.info.name);
        println!("\t announce url : {:?}", meta_info.announce);
        println!("\t files :");
        meta_info.print_files();
        return Ok(meta_info);
    }
}
