use serde::{Deserialize, Serialize};


#[derive(Serialize, Deserialize, Debug)]
pub struct PeerOptions {
    per_peer_inflight_count: usize,
    per_peer_active_request_count: usize,
    per_piece_active_peer_count: usize,
    block_request_timeout: usize, //seconds
    piece_request_timeout: usize //seconds
}
//Default values for client if not provided in the setting.yaml file:
impl Default for PeerOptions {
    fn default() -> Self {
        PeerOptions { per_peer_inflight_count:25, per_peer_active_request_count: 5, per_piece_active_peer_count: 5, block_request_timeout: 4, piece_request_timeout: 60 }
    }
}