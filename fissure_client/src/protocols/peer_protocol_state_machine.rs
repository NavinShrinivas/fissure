use crate::managers::piece_manager::PieceRequestManager;
use crate::models::peer_messages::{PeerCodec, PeerMessage};
use crate::protocols::peer_handshake::{PeerConnection};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::{ Framed};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{ AtomicU64};
use std::time::{Duration};
use tokio::sync::{Notify, RwLock};
use rand::prelude::*;

use log::{debug, error, info};


pub struct PeerFSMNetworkState{
    pub pending_active_request_count: AtomicU64,
    pub inflight_messages: AtomicU64, //number of block reqs in flight
    pub active_requests : RwLock<HashMap<u64, Arc<PieceRequestManager>>>,
    pub state_update: Notify
}


pub async fn state_machine(
    mut peer_conn: PeerConnection
) {

    let peer_network_state = Arc::new(PeerFSMNetworkState{
        pending_active_request_count: AtomicU64::new(0),
        inflight_messages: AtomicU64::new(0),
        active_requests: RwLock::new(HashMap::new()),
        state_update: Notify::new()
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
        let mut notification = writer_network_state.state_update.notified();

        loop {
            loop {
                let (is_choked, has_useful, is_interested) = {
                    let s = writer_peer_state.read().await;
                    (s.state.peer_choking, s.state.has_useful_pieces, s.state.am_interested)
                };

                if has_useful && !is_interested {
                    //We dont bother whether we are choked or not, just indicate we are interested if so
                    if framed_writer.send(PeerMessage::Interested).await.is_ok() {
                        writer_peer_state.write().await.state.am_interested = true;
                    } else {
                        log::warn!("FSM: Could not send interested request.");
                        return;
                    }
                }

                let mut made_progress = false;

                // Top off active pieces up to 5
                let active_pieces_count = writer_network_state.pending_active_request_count.load(SeqCst);
                if !is_choked && active_pieces_count < 5 {
                    let peer_id = writer_peer_state.read().await.state.peer_id.clone().unwrap();
                    if let Some(_req) = writer_manager_handler.request_piece(peer_id).await {
                        writer_network_state.active_requests.write().await.insert(_req.index, _req);
                        writer_network_state.pending_active_request_count.fetch_add(1, SeqCst);
                        made_progress = true;
                    }
                }

                // Top off pipelining window up to 25 requests
                let inflight_count = writer_network_state.inflight_messages.load(SeqCst);
                if inflight_count < 25 && !is_choked {
                    let available_window = (25 - inflight_count) as usize;
                    let total_items = writer_network_state.active_requests.read().await.len();

                    if total_items > 0 && available_window > 0 {
                        let target_index = {
                            let mut rng = rand::thread_rng();
                            rng.gen_range(0..total_items)
                        };

                        let guard = writer_network_state.active_requests.read().await;
                        if let Some((_, value)) = guard.iter().nth(target_index) {
                            let c = value.clone();
                            drop(guard);

                            let req_pieces = c.request_block(available_window as u32).await;
                            if !req_pieces.is_empty() {
                                for p in req_pieces {
                                    let msg = PeerMessage::Request {
                                        piece_index: c.index,
                                        block_offset: p.0,
                                        length: p.1,
                                    };

                                    if framed_writer.send(msg).await.is_ok() {
                                        writer_network_state.inflight_messages.fetch_add(1, SeqCst);
                                        made_progress = true;
                                    } else {
                                        log::error!("FSM: Could not send new block request.");
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }

                if !made_progress {
                    break;
                }
            }

            //We continue past the outer loop
            //Either when we get the notification of some state change
            //or every 2 seconds
            tokio::select! {
                _ = notification => {},
                _ = request_ticker.tick() => {},
            }

            notification = writer_network_state.state_update.notified();
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
                            log::info!("Got bitfiled form peer!");
                            reader_manager_handler.add_bitfield(peer_id.clone(), bitfield).await;
                            let has_useful = reader_manager_handler.has_useful_pieces(peer_id.clone()).await;{
                                let mut state_guard = reader_peer_state.write().await;
                                state_guard.state.has_useful_pieces = has_useful;
                            }
                            reader_network_state.state_update.notify_one();
                        }
                        PeerMessage::Request { piece_index, block_offset, length } => {
                            info!("Recived request for piece, currently not implemented. Ignoring.");
                            //TODO
                        },
                        PeerMessage::Piece { piece_index, block_offset, data } => {
                            log::info!("Received a block for piece {}", piece_index);
                            reader_network_state.inflight_messages.fetch_sub(1, SeqCst);
                            
                            let piece_manager = {
                                let guard = reader_network_state.active_requests.read().await;
                                guard.get(&piece_index).cloned() // Clones the Arc pointer, not the whole manager
                            };

                            if let Some(mgr) = piece_manager {
                                let hash_matched = mgr.block_recvied(block_offset as usize, data).await;
                                if hash_matched{
                                    reader_manager_handler.piece_finish(peer_id.clone(), piece_index).await;
                                }
                            } else {
                                log::warn!("Received block for untracked piece index: {}", piece_index);
                            }
                            
                            reader_network_state.state_update.notify_one();
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
