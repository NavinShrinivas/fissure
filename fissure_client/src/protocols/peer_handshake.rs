use std::time::Instant;

use crate::managers::torrent_manager::TorrentManager;
use crate::protocols::tracker::Peer;
use bitvec::vec::BitVec;
use log::warn;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Client - Us | Remote Peer - Others
pub struct PeerConnection {
    pub conn: Option<TcpStream>,
    pub state: PeerState,
    pub torrent_manager: TorrentManager,
}

#[derive(Clone, Debug)]
#[allow(unused)]
pub struct PeerState{
    //Things used inside the state machine
    pub peer_id: Option<String>,
    pub am_choking: bool, // we are choking them
    pub am_interested: bool, //we are interested in what they have
    pub peer_choking: bool, // The connected peer is choking us
    pub peer_interested: bool, //the peer is interested in what we have
    pub keep_alive: Instant,
    pub has_useful_pieces: bool,
}

impl PeerConnection {
    pub fn init_connection(
        c: TcpStream,
        pi: Option<String>,
        torrent_manager: TorrentManager
    ) -> Self {
        // log::debug!("bit field calc : {} {}", bitfield_size, (((bitfield_size as f64 / 8.0) as f64).ceil() * 8.0));
        //The cield bitfield calculation is for possible padding, we need not worry about that
        //As they will not be requested from the job sched side
        PeerConnection {
            conn: Some(c),
            torrent_manager: torrent_manager.clone(),
            //A separate bit field for each peer :
            state : PeerState{
                has_useful_pieces: true,
                peer_id: pi,
                am_choking: true,
                peer_choking: true,
                peer_interested: false,
                am_interested: false, //we should start out as not intrested and based in bitfield we decide
                keep_alive: Instant::now(),
            },
        }
    }

    pub async fn peer_connection_from_peer_meta(
        peer_meta: &Peer,
        torrent_manager: TorrentManager,
        peer_id: String
    ) -> Option<Self> {
        log::debug!("\tconnecting to {}:{}..", peer_meta.ip, peer_meta.port);
        
        let mut handshake = Vec::with_capacity(68);
        handshake.push(19u8);                                         // pstrlen
        handshake.extend_from_slice(b"BitTorrent protocol");          // pstr
        handshake.extend_from_slice(&[0u8; 8]);                       // reserved bytes
        handshake.extend_from_slice(&torrent_manager.meta.info_hash);    // 20-byte info hash
        handshake.extend_from_slice(peer_id.as_bytes());              // 20-byte local peer id

        let mut stream = match TcpStream::connect(format!("{}:{}", peer_meta.ip, peer_meta.port)).await {
            Ok(s) => s,
            Err(_) => {
                //there will be a lot of bad peers in a torrent network, no point in polluting the logs with it.
                //warn!("Failed to connect to {}:{}: {}", peer_meta.ip, peer_meta.port, e);
                return None; 
            }
        };

        if stream.write_all(&handshake).await.is_err() {
            warn!("Failed to send handshake to {}:{}", peer_meta.ip, peer_meta.port);
            return None;
        }

        let mut pstr_len_buf = [0u8; 1];
        if stream.read_exact(&mut pstr_len_buf).await.is_err() {
            warn!("Peer disconnected before sending handshake length.");
            return None;
        }
        let pstr_len = pstr_len_buf[0] as usize;
        if pstr_len == 0 {
            warn!("Invalid protocol string length from peer.");
            return None;
        }

        let remaining_handshake_len = pstr_len + 8 + 20 + 20; 
        let mut remaining_data = vec![0u8; remaining_handshake_len];
        
        if stream.read_exact(&mut remaining_data).await.is_err() {
            warn!("Failed to read complete handshake payload from peer.");
            return None;
        }

        let peer_id_start = pstr_len + 8 + 20; 
        let peer_id_bytes = &remaining_data[peer_id_start..peer_id_start + 20];
        
        let res_peer_id = String::from_utf8_lossy(peer_id_bytes).into_owned();
        log::debug!("Handshake completed with peer: {:?}", res_peer_id);


        Some(PeerConnection::init_connection(
            stream,
            Some(res_peer_id),
            torrent_manager.clone()
        ))
    } 
}
