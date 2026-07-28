/*!
    decodes the bencoded .torrent files and splits them into the properties
    defined in the initial bit torrent protocol (https://www.bittorrent.org/beps/bep_0003.html)
*/

use byte_unit::{self, UnitType};
use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
// If I'd have to match to a different name
// #[serde(rename = "piece length")]

//===================torrent file==================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileInfo {
    pub length: u64,       // Size of each file
    pub path: Vec<String>, // Path of the file, not sure if relative or not. Should be parseable by PathBuf
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Info {
    //In the single file case, the name key is the name of a file, in the multiple file case, it's the name of a directory.
    pub name: String, // The file name/path to store...only recommended
    #[serde(rename = "piece length")]
    pub piece_length: u64, //Size of each piece the file is split into
    #[serde(rename = "pieces")]
    #[serde(with = "serde_bytes")]
    pub pieces_hash: Vec<u8>, // SHA-1 of all the piece stiched together, each sha-1 is 20 in length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>, // Exists only for single file downloads, tells length of file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>, // Exists only if multi file downloads
    //The ordering of the files is criticl as the torrent just
    //considers it as one big contigous stream of bites
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetaInfo {
    pub announce: String, // Contains the url for the tracker
    #[serde(rename = "announce-list")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub announce_list: Option<Vec<Vec<String>>>, //Is the the way multi-tracker torrents are shared, this field get priority over the old one
    pub info: Info,
}

impl MetaInfo {
    pub fn files(&self) -> Vec<FileInfo> {
        //We wont have both length and files at the same time
        //length for single file torrents
        //files for multi file torrents
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
            let bytes = byte_unit::Byte::from_u64(content.length);

            let adjusted_bytes = bytes.get_appropriate_unit(UnitType::Binary);

            let two_digit = format!("{adjusted_bytes:.2}");

            info!(
                "\t\t {}. path : {:?}, size :{}",
                index + 1,
                path_buf,
                two_digit
            );
        }
    }
    /**
     * This function works for multi-files and single file 
     * torrent as we are summing up file sizes and not
     * depending on info from the torrent file
     */
    pub fn download_size(&self) -> u64 {
        let mut tot_size: u64 = 0;
        for i in self.files() {
            tot_size += i.length;
        }
        return tot_size;
    }
    pub fn get_piece_hash(&self, index: usize) -> &[u8] {
        let start_index = index * 20;
        let end_index = start_index + 20;
        
        &self.info.pieces_hash[start_index..end_index]
    }
    pub fn get_number_of_pieces(&self) -> usize{
        let total_size = self.download_size();
        let piece_len = self.info.piece_length;
        ((total_size + piece_len - 1) / piece_len) as usize //ciel to include the non full piece
    }
    pub fn get_piece_length(&self, index: usize) -> u64 {
        let total_size = self.download_size();
        let piece_len = self.info.piece_length;
        let num_pieces = self.get_number_of_pieces();

        // Safety check: ensure index is within valid range
        if index >= num_pieces {
            return 0; 
        }

        // Check if this is the last piece
        if index == num_pieces - 1 {
            let remainder = total_size % piece_len;
            // If remainder is 0, the last piece is a full-sized piece
            if remainder == 0 {
                piece_len
            } else {
                remainder
            }
        } else {
            // All other pieces are the standard piece length
            piece_len
        }
    }

    pub fn get_standard_piece_len(&self) -> u64{ 
        return self.info.piece_length;
    }
}
//==================================================


//==================================================
