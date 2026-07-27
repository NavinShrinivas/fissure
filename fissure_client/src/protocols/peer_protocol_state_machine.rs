use crate::managers::piece_manager::PieceRequestManager;
use crate::managers::torrent_manager::TorrentManager;
use crate::models::peer_messages::{PeerCodec, PeerMessage};
use crate::protocols::peer_handshake::{PeerConnection};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::{ Framed};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{ AtomicU64};
use std::time::{Duration};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{Notify, RwLock};
use rand::prelude::*;

use log::{debug, error, info};

const MAX_INFLIGHT: u64 = 25;
const MAX_ACTIVE_PIECES: u64 = 5;

pub struct PeerFSMNetworkState{
    pub pending_active_request_count: AtomicU64,
    pub inflight_messages: AtomicU64, //number of block reqs in flight
    pub active_requests : RwLock<HashMap<u64, Arc<PieceRequestManager>>>,
    pub state_update: Notify, //real state changes: choke/unchoke/interest/bitfield/have
    pub block_acked: Notify, //fired on every received block - only used to top off the request window
}

/// Requests more blocks from a single (randomly chosen) active piece, up to the inflight cap.
/// Returns `Some(made_progress)` normally, or `None` if the socket write failed (caller should stop).
async fn top_off_inflight_window(
    network_state: &Arc<PeerFSMNetworkState>,
    framed_writer: &mut Framed<OwnedWriteHalf, PeerCodec>,
    is_choked: bool,
) -> Option<bool> {
    if is_choked {
        return Some(false);
    }

    let inflight_count = network_state.inflight_messages.load(SeqCst);
    if inflight_count >= MAX_INFLIGHT {
        return Some(false);
    }
    let available_window = (MAX_INFLIGHT - inflight_count) as usize;

    let mut candidates: Vec<Arc<PieceRequestManager>> = {
        let guard = network_state.active_requests.read().await;
        guard.values().cloned().collect()
    };
    if candidates.is_empty() {
        return Some(false);
    }
    candidates.shuffle(&mut rand::thread_rng());

    // Try each active piece in turn - a piece with no requestable blocks left
    // (nearly complete, all remaining blocks already in flight) must not stop
    // us from topping off the window from a different active piece.
    for c in candidates {
        let req_pieces = c.request_block(available_window as u32).await;
        if req_pieces.is_empty() {
            continue;
        }

        for p in req_pieces {
            let msg = PeerMessage::Request {
                piece_index: c.index,
                block_offset: p.0,
                length: p.1,
            };

            if framed_writer.send(msg).await.is_ok() {
                network_state.inflight_messages.fetch_add(1, SeqCst);
            } else {
                log::error!("FSM: Could not send new block request.");
                return None;
            }
        }
        return Some(true);
    }

    Some(false)
}

/// Lightweight path for a block-ack wakeup: only top off the request window.
/// Does NOT re-check interest state or acquire new pieces - that's the full pass's job.
async fn run_window_topoff(
    peer_state: &Arc<RwLock<PeerConnection>>,
    network_state: &Arc<PeerFSMNetworkState>,
    framed_writer: &mut Framed<OwnedWriteHalf, PeerCodec>,
) -> bool {
    let is_choked = peer_state.read().await.state.peer_choking;
    loop {
        match top_off_inflight_window(network_state, framed_writer, is_choked).await {
            Some(true) => continue,
            Some(false) => return true,
            None => return false,
        }
    }
}

/// Full pass: send Interested if useful, top off active pieces up to the cap, and top off the
/// request window. Runs on real state changes and on the periodic safety-net tick.
async fn run_full_pass(
    peer_state: &Arc<RwLock<PeerConnection>>,
    manager: &TorrentManager,
    network_state: &Arc<PeerFSMNetworkState>,
    framed_writer: &mut Framed<OwnedWriteHalf, PeerCodec>,
) -> bool {
    loop {
        let (is_choked, has_useful, is_interested) = {
            let s = peer_state.read().await;
            (s.state.peer_choking, s.state.has_useful_pieces, s.state.am_interested)
        };

        if has_useful && !is_interested {
            if framed_writer.send(PeerMessage::Interested).await.is_ok() {
                peer_state.write().await.state.am_interested = true;
            } else {
                log::warn!("FSM: Could not send interested request.");
                return false;
            }
        }

        let mut made_progress = false;

        let active_pieces_count = network_state.pending_active_request_count.load(SeqCst);
        if !is_choked && active_pieces_count < MAX_ACTIVE_PIECES {
            let peer_id = peer_state.read().await.state.peer_id.clone().unwrap();
            if let Some(req) = manager.request_piece(peer_id).await {
                network_state.active_requests.write().await.insert(req.index, req);
                network_state.pending_active_request_count.fetch_add(1, SeqCst);
                made_progress = true;
            }
        }

        match top_off_inflight_window(network_state, framed_writer, is_choked).await {
            Some(progressed) => made_progress = made_progress || progressed,
            None => return false,
        }

        if !made_progress {
            return true;
        }
    }
}


pub async fn state_machine(
    mut peer_conn: PeerConnection
) {

    let peer_network_state = Arc::new(PeerFSMNetworkState{
        pending_active_request_count: AtomicU64::new(0),
        inflight_messages: AtomicU64::new(0),
        active_requests: RwLock::new(HashMap::new()),
        state_update: Notify::new(),
        block_acked: Notify::new(),
    });
    // This moves ownership of the socket out, leaving the rest of the struct intact.
    let conn = peer_conn.conn.take().expect("Connection already used up.");
    let (reader, writer) = conn.into_split();
    let torrent_manager = peer_conn.torrent_manager.clone();
    let share_peer_state = Arc::new(RwLock::new(peer_conn));

    debug!("PeerFSN: Starting protocol state machine for one peer");

    let peer_id_key = share_peer_state.read().await.state.peer_id.clone()
        .expect("Peer ID must be assigned before starting FSM loop");

    let watchdog_manager_handler = torrent_manager.clone();
    let watchdog_peer_network_state = peer_network_state.clone();
    let watchdog = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;
            let still_active_requests = match watchdog_manager_handler

                .update_and_get_active_request_count(peer_id_key.clone()).await 
            {
                Some(active) => active,
                None => {
                    log::error!("PeerFSM: Some how we arent being tracked by manager, quitting watchdog.");
                    return;
                }
            };

            let new_count = still_active_requests.len() as u64;
            watchdog_peer_network_state.pending_active_request_count.store(new_count, SeqCst);

            // Synchronize the local active request tracking map
            let mut requests = watchdog_peer_network_state.active_requests.write().await;
            let old_len = requests.len();
            
            requests.retain(|&k, _| still_active_requests.iter().any(|t| t.0 == k as usize));
            
            if requests.len() != old_len {
                log::info!("Watchdog pruned stale pieces. Notifying writer.");
                watchdog_peer_network_state.state_update.notify_one();
            }
        }
    }); 

    let writer_manager_handler = torrent_manager.clone();
    let writer_network_state = peer_network_state.clone();
    let writer_peer_state = share_peer_state.clone();

    let writer_join_handle = tokio::spawn(async move {
        let mut framed_writer = Framed::new(writer, PeerCodec::new());

        let mut request_ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        let state_notified = writer_network_state.state_update.notified();
        let block_notified = writer_network_state.block_acked.notified();
        tokio::pin!(state_notified);
        tokio::pin!(block_notified);
        
        // Startup pass: send Interested / kick off initial piece + block requests.
        if !run_full_pass(&writer_peer_state, &writer_manager_handler, &writer_network_state, &mut framed_writer).await {
            return;
        }

        loop {
            enum Wake { State, Block, Tick }

            let wake = tokio::select! {
                _ = &mut state_notified => Wake::State,
                _ = &mut block_notified => Wake::Block,
                _ = request_ticker.tick() => Wake::Tick,
            };

            let ok = match wake {
                Wake::State => {
                    state_notified.set(writer_network_state.state_update.notified());
                    run_full_pass(&writer_peer_state, &writer_manager_handler, &writer_network_state, &mut framed_writer).await
                }
                Wake::Block => {
                    block_notified.set(writer_network_state.block_acked.notified());
                    // Lightweight path - a block being acked only ever needs the request
                    // window topped off, not a full re-evaluation (new piece / interest state).
                    run_window_topoff(&writer_peer_state, &writer_network_state, &mut framed_writer).await
                }
                Wake::Tick => {
                    run_full_pass(&writer_peer_state, &writer_manager_handler, &writer_network_state, &mut framed_writer).await
                }
            };

            if !ok {
                return;
            }
        }
    });


    let reader_manager_handler = torrent_manager.clone();
    let reader_network_state = peer_network_state.clone();
    let reader_peer_state = share_peer_state.clone();

    let reader_join_handle = tokio::spawn(async move{
        let mut stream = Framed::new(reader, PeerCodec::new());
        let peer_id = reader_peer_state.read().await.state.peer_id.clone().unwrap();
        while let Some(msg) = stream.next().await{
            match msg {
                Ok(peer_msg) => {
                    match peer_msg{
                        PeerMessage::KeepAlive => {
                            //Do Nothing - TODO
                            continue;
                        }
                        PeerMessage::Choke => {
                            reader_peer_state.write().await.state.peer_choking = true;
                            reader_network_state.inflight_messages.store(0, SeqCst);
                            reader_network_state.state_update.notify_one();

                        }
                        PeerMessage::Unchoke => {
                            reader_peer_state.write().await.state.peer_choking = false;
                            reader_network_state.state_update.notify_one();
                        }
                        PeerMessage::Interested => {
                            reader_peer_state.write().await.state.peer_interested = true;
                            reader_network_state.state_update.notify_one();
                        }
                        PeerMessage::NotInterested => {
                            reader_peer_state.write().await.state.peer_interested = false;
                            reader_network_state.state_update.notify_one();
                        }
                        PeerMessage::Have { piece_index } => {
                            reader_manager_handler.update_bitfield(peer_id.clone(), piece_index).await;
                        }
                        PeerMessage::Bitfield { bitfield } => {
                            log::debug!("Got bitfiled form peer!");
                            reader_manager_handler.add_bitfield(peer_id.clone(), bitfield).await;
                            let has_useful = reader_manager_handler.has_useful_pieces(peer_id.clone()).await;{
                                let mut state_guard = reader_peer_state.write().await;
                                state_guard.state.has_useful_pieces = has_useful;
                            }
                            reader_network_state.state_update.notify_one();
                        }
                        PeerMessage::Request { piece_index: _, block_offset: _, length: _ } => {
                            info!("Recived request for piece, currently not implemented. Ignoring.");
                            //TODO
                        },
                        PeerMessage::Piece { piece_index, block_offset, data } => {
                            log::debug!("Received a block for piece {}", piece_index);
                            reader_network_state.inflight_messages.fetch_sub(1, SeqCst);
                            
                            let piece_manager = {
                                let guard = reader_network_state.active_requests.read().await;
                                guard.get(&piece_index).cloned() // Clones the Arc pointer, not the whole manager
                            };

                            if let Some(mgr) = piece_manager {
                                let hash_matched = mgr.block_recvied(block_offset as usize, data).await;
                                if hash_matched{
                                    reader_manager_handler.piece_finish(peer_id.clone(), piece_index).await;
                                    // Free the active-piece slot immediately - don't wait for the
                                    // periodic watchdog to notice, or the peer will stall believing
                                    // it's still maxed out on MAX_ACTIVE_PIECES.
                                    reader_network_state.active_requests.write().await.remove(&piece_index);
                                    reader_network_state.pending_active_request_count.fetch_sub(1, SeqCst);
                                    // A piece finished - that's a real state change (frees an active-piece
                                    // slot), so it warrants a full pass to pick up a new piece promptly.
                                    reader_network_state.state_update.notify_one();
                                } else {
                                    // Just one block acked - only the request window needs topping off.
                                    reader_network_state.block_acked.notify_one();
                                }
                            } else {
                                log::warn!("Received block for untracked piece index: {}", piece_index);
                            }
                        }
                       
                        _ => {
                            info!("Unsupported messages recieved from peeer.");
                        }
                    }

                },
                Err(e) => {
                    error!("Error while reading next message from stream : {}", e);
                    return;
                }

            }
        }
    });

    // Instead of raw join_all, monitor both tasks. 
    // If one finishes (or crashes), we immediately proceed to cleanup.
    tokio::select! {
        res = writer_join_handle => {
            match res {
                Ok(_) => info!("Writer task exited naturally."),
                Err(e) => {
                    error!("Writer task panicked/errored: {}", e)
                }
            }
        }
        res = reader_join_handle => {
            match res {
                Ok(_) => info!("Reader task exited naturally."),
                Err(e) => error!("Reader task panicked/errored: {}", e),
            }
        }
    }

    //TODO = we shold call RemovePeer in the manager.

    // Abort the watchdog timer task so it stops spinning in the background
    watchdog.abort();



    info!("Peer FSM state machine terminated completely and cleanly.");

}
