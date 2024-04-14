use crate::models::torrent_meta::MetaInfo;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct BeeEncoderErr {
    error_string: String,
}

impl fmt::Display for BeeEncoderErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.error_string)
    }
}

impl Error for BeeEncoderErr {}
impl MetaInfo {
    pub fn info_bencoded_binary_form(&self) -> Result<Vec<u8>, BeeEncoderErr> {
        // process input message
        match serde_bencoded::to_vec(&self.info){
            Ok(v) => { return Ok(v)},
            Err(e) => {
                return Err(BeeEncoderErr{
                    error_string : "[ERROR] Error encoding info value back to bencoded.".to_string()+&e.to_string()
                })
            }
        };
    }
}

