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
pub struct PeerConnection {
    conn: TcpStream,
    choked: bool,
    bitfield: Vec<String>, // 0 for not available, 1 for avail. Using Strings we can avoid some bit
    // manip logic
    info_hash: [u8; 20],
    peer_id: Option<String>,
}

impl PeerConnection {
    pub async fn peer_connection_from_peer_meta(
        peer_meta: &Peer,
        working_torrent: usize,
        client_state_arc: Arc<RwLock<ClientState>>,
    ) -> Self {
        //Pretty much does handshake
        let client_state = client_state_arc.read().await;
        let client_torrent_meta: &ClientTorrentMeta =
            client_state.torrents.get(working_torrent).unwrap();
        let mut handshake_str: String = String::new();
        // we need the string to be completley hex, we decode to binary and shoot it to TCP
        // 19 decimal in hex : 0x13
        handshake_str = format!("{}{}", handshake_str, "13");
        handshake_str = format!("{}{}", handshake_str, hex::encode("BitTorrent protocol"));
        println!("{:?}", peer_meta);
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
        println!("{}", handshake_str);
        match TcpStream::connect(format!("{}:{}", peer_meta.ip, peer_meta.port)) {
            Ok(mut stream) => {
                println!("Successfully connected to server in port 3333");
                stream
                    .write(hex::decode(handshake_str).unwrap().as_slice())
                    .unwrap();
                println!("Sent handshake...");
                let mut data = [0; 1];
                match stream.read_exact(&mut data) {
                    Ok(_) => {
                        let pstr_len = Cursor::new(data).read_u8().unwrap() as usize;
                        println!("handling handshake response...{}", pstr_len);
                        if pstr_len != 0 {
                            let mut data = vec![0u8; 48 + pstr_len];
                            stream.read(&mut data);
                            println!(
                                "infohash : {:?}",
                                hex::encode(&data[pstr_len + 8..pstr_len + 8 + 20])
                            );
                            println!("peerid : {:?}", hex::encode(&data[pstr_len + 8 + 20..]));
                        } else {
                            panic!("Message length should not be 0...");
                        }
                        return PeerConnection {
                            conn: stream,
                            choked: true,
                            bitfield: Vec::new(),
                            info_hash: client_torrent_meta.info_hash,
                            peer_id: peer_meta.peer_id.clone(),
                        };
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
