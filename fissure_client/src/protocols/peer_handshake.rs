use crate::models::client_meta::ClientState;
use crate::models::client_meta::ClientTorrentMeta;
use crate::models::torrent_meta::Peer;
use byteorder;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use hex;
use std::io::Cursor;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

enum MessageID {
    MsgChoke,
    MsgUnchoke,
    MsgInterested,
    MsgNotInterested,
    MsgHave,
    MsgBitfield,
    MsgRequest,
    MsgPiece,
    MsgCancel,
}

impl MessageID {
    pub fn ser(&self) -> i32 {
        match self {
            Self::MsgChoke => 0,
            Self::MsgUnchoke => 1,
            Self::MsgInterested => 2,
            Self::MsgNotInterested => 3,
            Self::MsgHave => 4,
            Self::MsgBitfield => 5,
            Self::MsgRequest => 6,
            Self::MsgPiece => 7,
            Self::MsgCancel => 8,
        }
    }
}

/// Should be sent into future workers along with channels for piece_work
/// Also holds metadata about each connection, We need to build out an FSM out of this state.
/// Client - Us | Remote Peer - Others
pub struct PeerConnection {
    pub conn: TcpStream,
    pub bitfield: Vec<String>, // 0 for not available, 1 for avail. Using Strings we can avoid some bit
    pub info_hash: [u8; 20],
    pub peer_id: Option<String>,
    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,
    pub keep_alive: Instant,
}

impl PeerConnection {
    pub fn init_connection(c: TcpStream, ih: [u8; 20], pi: Option<String>, bitfield_size : u64) -> Self {
        PeerConnection {
            conn: c,
            bitfield: vec!["0".to_string(); (((bitfield_size/8) as f64).ceil()*8.0) as usize],
            info_hash: ih,
            peer_id: pi,
            am_choking: true,
            peer_choking: true,
            peer_interested: false,
            am_interested: false,
            keep_alive: Instant::now(),
        }
    }
    pub async fn peer_connection_from_peer_meta(
        peer_meta: &Peer,
        working_torrent: usize,
        client_state_arc: Arc<RwLock<ClientState>>,
    ) -> Self {
        //Pretty much does handshake
        println!("connecting..");
        let client_state = client_state_arc.read().await;
        let client_torrent_meta: &ClientTorrentMeta =
            client_state.torrents.get(working_torrent).unwrap();
        let mut handshake_str: String = String::new();
        // we need the string to be completley hex, we decode to binary and shoot it to TCP
        // 19 decimal in hex : 0x13
        handshake_str = format!("{}{}", handshake_str, "13");
        handshake_str = format!("{}{}", handshake_str, hex::encode("BitTorrent protocol"));
        // println!("{:?}", peer_meta);
        // 8 bytes being 0 in hex is 16 0's
        handshake_str = format!("{}{}", handshake_str, "0000000000000000");
        handshake_str = format!(
            "{}{}",
            handshake_str,
            hex::encode(client_torrent_meta.info_hash)
        );
        handshake_str = format!(
            "{}{}",
            handshake_str,
            hex::encode(client_state.peer_id.clone())
        );
        // println!("{}", handshake_str);
        match TcpStream::connect(format!("{}:{}", peer_meta.ip, peer_meta.port)) {
            Ok(mut stream) => {
                // println!("Successfully connected to server in port 3333");
                stream
                    .write(hex::decode(handshake_str).unwrap().as_slice())
                    .unwrap();
                // println!("Sent handshake...");
                let mut data = [0; 1];
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .expect("Error setting timeout on TCP buffer read.");
                match stream.read_exact(&mut data) {
                    Ok(_) => {
                        let pstr_len = Cursor::new(data).read_u8().unwrap() as usize;
                        // println!("handling handshake response...{}", pstr_len);
                        if pstr_len != 0 {
                            let mut data = vec![0u8; 48 + pstr_len];
                            stream.read(&mut data).unwrap();
                            // println!(
                            //     "infohash : {:?}",
                            //     hex::encode(&data[pstr_len + 8..pstr_len + 8 + 20])
                            // );
                            let res_peer_id = String::from_utf8(data[pstr_len + 8 + 20..].to_vec())
                                .unwrap_or("".to_string());
                            // if peer_meta.peer_id != Some(res_peer_id.clone()){
                            //     stream.shutdown(std::net::Shutdown::Both);
                            //     panic!("Invalid peer id, aborting connection.")
                            // }
                            println!("Connected to : {:?}", res_peer_id);
                        } else {
                            panic!("Message length should not be 0...");
                        }
                        return PeerConnection::init_connection(
                            stream,
                            client_torrent_meta.info_hash,
                            peer_meta.peer_id.clone(),
                            client_torrent_meta.raw_torrent.info.length.unwrap()/client_torrent_meta.raw_torrent.info.piece_length
                        );
                    }
                    Err(e) => {
                        panic!("Failed to receive data: {}", e);
                    }
                }
            }
            Err(e) => {
                panic!("Failed to connect: {}", e);
            }
        };
    }
}
