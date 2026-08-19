use std::{collections::{HashMap, HashSet}, time::{Duration, Instant}};

use bitvec::prelude::*;
use tokio::sync::{Notify, mpsc, oneshot, watch};
use std::sync::Arc;

use crate::managers::{client_manager::ClientTorrentMetaInfo, disk_manager::DiskManager, piece_manager::PieceRequestManager, torrent_manager::TorrentManagerMessage::{AddBitFeild, GetActivePeers, GetDataWithLength, GetHaveMessageProgress, GetPieceManager, GetTorrentStats, HasUsefulPieces, PieceFinish, RequestWork, UpdateAndGetActiveRequestCount, UpdateBitField}};
use crate::models::peer_settings::PeerOptions;


#[derive(Clone, Debug)]
pub struct TorrentStats{
    pub downloaded: String,
    pub uploaded: String,
    pub left: String,
}
struct TorrentManagerActor{
    pieces_frequency: Vec<usize>, //the values are the freq
    rarity_bucket: Vec<HashSet<usize>>, //the values are the peice index
    pub stats: TorrentStats,
    pub client_torrent_meta: Arc<ClientTorrentMetaInfo>, //ctmi never changes, Arc is enough
    pub peer_bitfields: HashMap<String, BitVec<u8,Msb0>>, //map of peerids and bitfields
    pub local_bitfield: BitVec<u8,Msb0>,

    // Key: Peer ID -> Value: Map of (Piece Index -> When it was assigned)
    pub peer_active_work: HashMap<String, Vec<(usize, Instant)>>,
    // Key: Piece Index -> Value: How many peers are currently working on it
    pub piece_concurrency_counts: HashMap<usize, usize>,

    pub active_pieces_manager: HashMap<usize, Arc<PieceRequestManager>>, //this PieceRequestManager will be shared by all peers in the above hashmap for the same p-index
    pub disk_manager:  DiskManager,
    pub downloaded_piece_index : Vec<u32>,
    //we use this as a log of all the pieces we have downloaded in order to enable the Have message sender
    //Dont need rwlock on it, because any contention causing old read is fine...
    pub peer_notifier_send: watch::Sender<u64>,
    pub recv: mpsc::Receiver<TorrentManagerMessage>,
    pub peer_options: PeerOptions,
}

impl TorrentManagerActor{
    fn new(torrent: Arc<ClientTorrentMetaInfo>, recv: mpsc::Receiver<TorrentManagerMessage>, download_path: String, peer_notifier_send: watch::Sender<u64>, peer_options: PeerOptions) -> Self{
        let num_of_p = torrent.clone().raw_torrent.get_number_of_pieces();
        let files = torrent.clone().files.clone();
        let std_piece_len = torrent.clone().raw_torrent.get_standard_piece_len();
        let total_size = torrent.clone().raw_torrent.download_size() as f64 / 1000000 as f64;
        Self{
            pieces_frequency: vec![0;num_of_p],
            rarity_bucket: vec![HashSet::new();256], //we arent ideally bothered by pieces held by more than 256 peers.
            stats: TorrentStats{
                downloaded: "0".to_string(), 
                uploaded: "0".to_string(), 
                left: total_size.to_string(), 
            },
            client_torrent_meta: torrent, 
            peer_bitfields: HashMap::new(), 
            local_bitfield: bitvec![u8, Msb0; 0; num_of_p as usize],
            peer_active_work: HashMap::new(),
            piece_concurrency_counts : (0..num_of_p).map(|i| (i, 0)).collect(),
            active_pieces_manager: HashMap::new(),
            disk_manager: DiskManager::new(download_path, files, std_piece_len),
            downloaded_piece_index: Vec::new(),
            peer_notifier_send,
            recv,
            peer_options
        }
    }
    fn update_freq_bitfield(&mut self, bitfield: &BitVec<u8,Msb0>, peer_id: &String) {
        let existing_bitfield = self.peer_bitfields.get(peer_id);
        log::debug!("{:?}{:?}", bitfield, existing_bitfield);

        for idx in bitfield.iter_ones() {
            // Skip pieces we already own completely
            if *self.local_bitfield.get(idx).as_deref().unwrap_or(&false) {
                continue;
            }

            // Only increment if the peer didn't already have this piece marked before
            let is_new_piece_for_peer = match existing_bitfield {
                Some(old_b) => !*old_b.get(idx).as_deref().unwrap_or(&false),
                None => true,
            };

            if is_new_piece_for_peer {
                let old_freq = self.pieces_frequency[idx];
                let new_freq = (old_freq + 1).min(255);

                self.rarity_bucket[old_freq.min(255)].remove(&idx);
                self.rarity_bucket[new_freq].insert(idx);
                self.pieces_frequency[idx] = new_freq;
            }
        }
        log::debug!("{:?}", self.rarity_bucket);
    }

    fn update_freq_have(&mut self, peer_id: &String, index: usize) {
        if *self.local_bitfield.get(index).as_deref().unwrap_or(&false) {
            return;
        }

        if let Some(peer_bitfield) = self.peer_bitfields.get_mut(peer_id) {
            if !*peer_bitfield.get(index).as_deref().unwrap_or(&false) {
                peer_bitfield.set(index, true);

                let old_freq = self.pieces_frequency[index];
                let new_freq = (old_freq + 1).min(255);

                self.rarity_bucket[old_freq.min(255)].remove(&index);
                self.rarity_bucket[new_freq].insert(index);
                self.pieces_frequency[index] = new_freq;
            }
        }
    }
    async fn run(mut self){
        while let Some(msg) = self.recv.recv().await{
            match msg {
                TorrentManagerMessage::AddPeer { peer_id, peer_bitfield , send} => {
                    self.update_freq_bitfield(&peer_bitfield, &peer_id);
                    self.peer_bitfields.insert(peer_id.clone(), peer_bitfield);
                    self.peer_active_work.insert(peer_id.clone(), Vec::new());
                    log::info!("Manager: Added new peer with peer id:{}", peer_id);
                    let _ = send.send(self.local_bitfield.clone()); //sending our bit right now
                },
                //TODO: Unused rn now, but im sure we need this.
                GetTorrentStats{ send } => {
                    let _ = send.send(self.stats.clone());
                },
                RequestWork{peer_id, reply} => {
                    //THIS BRANCH IS THE BRAINS FOR PIECE SELECTION ALGORITHM
                    //First see what all pieces this peer is already handling

                    let current_peer_bitfield = match self.peer_bitfields.get(&peer_id){
                        Some(b) => {
                            b
                        },
                        None => {
                            log::warn!("Peer bitfield doesnt exist for peer: {}, cannot assign pieces rn.", peer_id);
                            let _ = reply.send(None);
                            continue;
                        }
                    };

                    //INITIAL ALGO : 
                    //TODO

                    //USUAL ALGO - Rarest first : 
                    let req_piece = self.rarity_bucket.iter()
                    .flatten()
                    .find(|&&idx| {
                        let has_piece = match current_peer_bitfield.get(idx){
                            Some(t) => {
                                t == true
                            }, None => {
                                false
                            }
                        };
                        let already_downloaded = match self.local_bitfield.get(idx){
                            Some(t) => {
                                t == true
                            }, None => {
                                //TODO: maybe this branch return value is wrong
                                log::warn!("manager: piece selection logic, already donwloaded bitfield is resolving to an invalid index.");
                                false
                            }
                        };
                        let active_count_within_cap = match self.piece_concurrency_counts.get(&idx){
                            Some(len) => {
                                *len <= self.peer_options.per_piece_active_peer_count
                            },
                            None => {
                                //If the value is None of the key, meaning we are done downloading this piece
                                return false;
                            }
                        };
                        let this_peer_already_assigned_this_request = match self.peer_active_work.get(&peer_id){
                            Some(t) => {
                                let contains = t.iter().any(|t| t.0 == idx);
                                contains
                            }, None => {
                                //TODO : To check if returning false from this branch is fine
                                false
                            }
                        };
                        return has_piece && !already_downloaded && active_count_within_cap && !this_peer_already_assigned_this_request
                    })
                    .copied();

                    if req_piece.is_none(){
                        // log::error!("We couldnt compute a piece request in the torrent, despite the piece still not having been completed. It is possible all the available pieces are already active in atleast per_piece_active_peer_count peers.");
                        let _ = reply.send(None);
                        continue;
                    }
                    let index = req_piece.unwrap();
                
                    //ENDGAME ALGO - randomly without limits: 
                    //TODO

                    let piece_manager = match self.active_pieces_manager.get(&index){
                        Some(manager ) =>{
                            manager.clone()
                        }, 
                        None => {
                            let new_manager = Arc::new(PieceRequestManager::new(index as u64, self.client_torrent_meta.raw_torrent.get_piece_hash(index).into(), self.client_torrent_meta.raw_torrent.get_piece_length(index), self.peer_options.block_request_timeout));
                            self.active_pieces_manager.insert(index, new_manager.clone());
                            new_manager.clone()
                        }
                    };
                    match self.piece_concurrency_counts.get_mut(&index){
                        Some(v) => {
                            *v+=1 as usize;
                            //v.insert((peer_id, Instant::now()));
                        }, 
                        None => {
                            panic!("Somehow, the piece selection algorithm has selected a piece that isnt pending.");
                        }
                    }
                    match self.peer_active_work.get_mut(&peer_id){
                        Some(v) => {
                            v.push((index, Instant::now()));
                        },
                        None => {
                            panic!("We are trying to assign work to peer whom we arent tracking in the manager. But the peer is active as we have gotten a request from for pieces");
                        }
                    }
                    let _ = reply.send(Some(piece_manager));
                    log::debug!("Sent over a piece request to peer");


                },
                AddBitFeild{peer_id, peer_bitfield} => {
                    self.update_freq_bitfield(&peer_bitfield, &peer_id);
                    self.peer_bitfields.insert(peer_id, peer_bitfield);
                },
                UpdateBitField{peer_id, index} => {
                    self.update_freq_have(&peer_id, index);
                    self.peer_bitfields.get_mut(&peer_id).unwrap().set(index, true);

                },
                PieceFinish{ _peer_id: _, index}=>{
                    //we should set the local_bitfield that we have the piece, and we shold trigger a update_Freq_have with the current piece id
                    if self.local_bitfield.get(index).unwrap() == true{
                        //We have already gotten this news from some other peer
                        //we can just continue
                        continue;
                    }
                    self.local_bitfield.set(index, true);

                    // Completely strip the piece from rarity tracking since we own it
                    let current_freq = self.pieces_frequency[index];
                    self.rarity_bucket[current_freq.min(255)].remove(&index);
                    self.pieces_frequency[index] = 0; // Reset tracking state

                    //No point in tracking any conurrency count or active peers wokring on it, so remove it
                    self.piece_concurrency_counts.remove(&index);
                    for (_peer, work_map) in self.peer_active_work.iter_mut() {
                        work_map.retain(|(p_index, _)| *p_index != index);
                    }

                    //lets first free up peers so that they can work on newer pieces when we flush this to disk
                    for v in self.peer_active_work.values_mut(){
                        v.retain_mut(|v| v.0 != index);
                    }

                    let last_manager_copy = self.active_pieces_manager.remove(&index);
                    if let Some(last_piece_manager) = last_manager_copy {
                        log::debug!("starting persist of piece to disk");
                        let current_piece_len = last_piece_manager.clone().piece_length;
                        let flush_result = self.disk_manager.flush_piece_to_disk(last_piece_manager).await;
                        if flush_result {
                            log::debug!("Persisted piece to disk successfully.");
                            self.stats.downloaded = (self.stats.downloaded.parse::<f64>().unwrap() +  current_piece_len as f64 /1000000 as f64).to_string();
                            self.downloaded_piece_index.push(index as u32);
                            self.peer_notifier_send.send(self.downloaded_piece_index.len() as u64).unwrap();
                        } else {
                            log::error!("Failed to persist piece to disk.");
                        }
                    } 
                    //TODO = Trigger a flush of piece to disk by sending over the peice manager that is currently storing the piece in mem
                    //we should use the amove last_manager_copy to make it happen

                }
                UpdateAndGetActiveRequestCount{
                    peer_id,
                    reply
                } => {
                    let timeout = Duration::from_secs(self.peer_options.piece_request_timeout as u64);
                    let now = Instant::now();
                    let updated_list = match self.peer_active_work.get_mut(&peer_id){
                        Some(v) => {
                            v.retain(|(piece, time)| {
                                if self.local_bitfield.get(*piece).unwrap() == true{
                                    return false;
                                }
                                if now.duration_since(*time) > timeout {
                                    log::warn!("Piece timed out on peer : {}", peer_id);
                                    if let Some(count) = self.piece_concurrency_counts.get_mut(piece){
                                        if *count > 0 {*count -= 1}
                                    }
                                    return false;
                                }
                                return true;
                            });
                            v.clone()
                        },
                        None => {
                            log::error!("A peer we arent tracking is asking for its active work!");
                            let _ = reply.send(None);
                            continue;
                        }
                    };
                    let _ = reply.send(Some(updated_list));
                    
                },
                HasUsefulPieces{
                    peer_id, 
                    reply
                } => {
                    let peer_bf= self.peer_bitfields.get(&peer_id).unwrap().clone();
                    let mask = !(self.local_bitfield.clone()) & peer_bf;
                    if mask.count_ones() > 0{
                        let _ = reply.send(true);
                        continue;
                    }
                    let _ = reply.send(false);
                },
                GetActivePeers{
                    reply
                } => {
                    let active_peers = self.peer_bitfields.keys().cloned().collect::<Vec<String>>();
                    let _ = reply.send(Some(active_peers));
                },
                GetDataWithLength{
                    piece_index,
                    offset,
                    length,
                    reply
                } => {
                    let data = self.disk_manager.get_data_with_length(piece_index as u32, offset as u32, length as u32).await;
                    let _ = reply.send(data);
                },
                GetPieceManager{
                    piece_index,
                    reply
                } => {
                    //Used by the peer FSM to deliver blocks for pieces it no longer
                    //tracks locally (assignment timed out / another peer finished it).
                    //Returns None if the piece is already done and gone.
                    let _ = reply.send(self.active_pieces_manager.get(&piece_index).cloned());
                },
                GetHaveMessageProgress{
                    since, 
                    reply
                } => {
                    let progress = self.downloaded_piece_index[since as usize..].to_vec(); //only cloning new stuff
                    reply.send(Some(progress)).unwrap_or_else(|e| {
                        log::error!("Failed to send have message progress: {:?}", e);
                    });
                }
                //[TODO] => Add handlers for all other types of messages
                _ => {
                    log::error!("Unkown message recived in torrent manager : {:?}", msg)
                }
            }
        }
    }
}
#[derive(Debug)]
enum TorrentManagerMessage{
    AddPeer{ //DONE
        peer_id : String, 
        peer_bitfield: BitVec<u8,Msb0>,
        send: oneshot::Sender<BitVec<u8,Msb0>>
    }, 
    UpdateBitField{ //DONE
        peer_id: String, 
        index: usize //piece index not chunk
    },
    AddBitFeild{ //DONE
        peer_id: String, 
        peer_bitfield: BitVec<u8, Msb0>
    },
    #[allow(dead_code)]
    RemovePeer{ //TODO
        peer_id: String
    }, 
    PieceFinish{ //DONE- Partially
        _peer_id: String, 
        index: usize, //piece index, not chunk
    },
    RequestWork{//DONE
        peer_id: String, 
        reply: oneshot::Sender<Option<Arc<PieceRequestManager>>> //piece, not block
    },
    GetTorrentStats{ //TODO
        send: oneshot::Sender<TorrentStats>
    },
    UpdateAndGetActiveRequestCount{//Done
        peer_id: String, 
        reply: oneshot::Sender<Option<Vec<(usize, Instant)>>>
    },
    HasUsefulPieces{ //DONE
        peer_id: String,
        reply: oneshot::Sender<bool>
    },
    GetActivePeers{ //TODO
        reply: oneshot::Sender<Option<Vec<String>>>
    },
    GetDataWithLength{ //TODO
        piece_index: usize,
        offset: usize,
        length: usize,
        reply: oneshot::Sender<Option<Vec<u8>>>
    },
    GetPieceManager{ //DONE
        piece_index: usize,
        reply: oneshot::Sender<Option<Arc<PieceRequestManager>>>
    },
    GetHaveMessageProgress{ //TODO
        since: u64, //get all the pieces we have downloaded since this index
        reply: oneshot::Sender<Option<Vec<u32>>>
    }
}

#[derive(Clone, Debug)]
pub struct TorrentManager{
    send: mpsc::Sender<TorrentManagerMessage>,
    pub meta : Arc<ClientTorrentMetaInfo>,
    peer_notifier_recv: watch::Receiver<u64>,
    peer_options: PeerOptions,
}

impl TorrentManager{
    pub fn new(client_torrent_meta_info: ClientTorrentMetaInfo, download_path: String, peer_options: PeerOptions) -> Self{
        let (send, recv) = mpsc::channel(400);
        let (peer_notifier_send, peer_notifiler_recv) = watch::channel::<u64>(0);
        let arc_ctmi = Arc::new(client_torrent_meta_info);
        let actor = TorrentManagerActor::new(arc_ctmi.clone(), recv, download_path, peer_notifier_send, peer_options);
        tokio::spawn(async move{actor.run().await});
        TorrentManager {
            send,
            meta: arc_ctmi,
            peer_notifier_recv: peer_notifiler_recv,
            peer_options,
        }
    }

    pub fn get_peer_options(&self) -> PeerOptions {
        self.peer_options
    }

    pub async fn piece_finish(&self, peer_id: String, piece_index: u64) { 
        let _ = self.send.send(PieceFinish { _peer_id: peer_id, index: piece_index as usize }).await;
    }

    pub async fn get_torrent_stats(&self)->Option<TorrentStats>{
        let (tx,rx) = oneshot::channel::<TorrentStats>();
        let _ = self.send.send(TorrentManagerMessage::GetTorrentStats { send:tx}).await;
        rx.await.ok()
    }

    pub async fn get_active_peers(&self) -> Option<Vec<String>>{
        let (tx,rx) = oneshot::channel::<Option<Vec<String>>>();
        let _ = self.send.send(TorrentManagerMessage::GetActivePeers { reply: tx }).await;
        rx.await.ok().unwrap_or(None)
    }

    pub async fn add_new_peer(&self, bit_vec: BitVec<u8,Msb0>, peer_id: String) -> (BitVec<u8,Msb0>, watch::Receiver<u64>) {
        let (tx,rx) = oneshot::channel();
        let _ = self.send.send(TorrentManagerMessage::AddPeer { peer_id, peer_bitfield: bit_vec , send: tx}).await;
        (rx.await.unwrap_or_default(), self.peer_notifier_recv.clone())
    }

    pub async fn request_piece(&self, peer_id: String) -> Option<Arc<PieceRequestManager>>{
        let (tx,rx) = oneshot::channel::<Option<Arc<PieceRequestManager>>>();
        let _ = self.send.send(TorrentManagerMessage::RequestWork { peer_id, reply: tx }).await;
        match rx.await.ok(){
            None => None,
            Some(v) => v
        }

    }

    /**
     * returns back the list of active and valid
     * requets pieces index, the FSM should clear anything
     * from its active requsts list that is not in 
     * what this function returns
     */
    pub async fn update_and_get_active_request_count(&self, peer_id: String) -> Option<Vec<(usize, Instant)>>{
        let (tx,rx) = oneshot::channel::<Option<Vec<(usize, Instant)>>>();
        let _ = self.send.send(TorrentManagerMessage::UpdateAndGetActiveRequestCount { peer_id, reply : tx}).await;
        rx.await.unwrap_or(None)
    }

    pub async fn add_bitfield(&self,peer_id: String, bit_vec: BitVec<u8,Msb0>) -> bool{
        let _ = self.send.send(TorrentManagerMessage::AddBitFeild { peer_id: peer_id.clone(), peer_bitfield: bit_vec }).await;
        let (tx,rx) = oneshot::channel::<bool>();
        let _ = self.send.send(TorrentManagerMessage::HasUsefulPieces { peer_id, reply: tx }).await;
        rx.await.unwrap_or(false)
    }

    pub async fn update_bitfield(&self,peer_id: String, piece_index: u64){
        let _ = self.send.send(TorrentManagerMessage::UpdateBitField { peer_id, index: piece_index as usize }).await;
    }

    pub async  fn has_useful_pieces(&self, peer_id: String) -> bool {
        let (tx,rx) = oneshot::channel::<bool>();
        let _ = self.send.send(TorrentManagerMessage::HasUsefulPieces { peer_id, reply: tx }).await;
        rx.await.ok().unwrap()
    }

    pub async fn get_data_with_length(&self, piece_index: usize, offset: usize, length: usize) -> Option<Vec<u8>>{
        let (tx,rx) = oneshot::channel::<Option<Vec<u8>>>();
        let _ = self.send.send(TorrentManagerMessage::GetDataWithLength { piece_index, offset, length, reply: tx }).await;
        rx.await.ok().unwrap_or(None)
    }

    /**
     * Returns the shared piece manager for a piece if it is still
     * active anywhere in the torrent, None if the piece is done.
     * Lets the FSM deliver late-arriving blocks instead of dropping them.
     */
    pub async fn get_piece_manager(&self, piece_index: u64) -> Option<Arc<PieceRequestManager>>{
        let (tx,rx) = oneshot::channel::<Option<Arc<PieceRequestManager>>>();
        let _ = self.send.send(TorrentManagerMessage::GetPieceManager { piece_index: piece_index as usize, reply: tx }).await;
        rx.await.unwrap_or(None)
    }

    pub async fn get_have_message_progress(&self, since: u64) -> Option<Vec<u32>>{
        let (tx,rx) = oneshot::channel::<Option<Vec<u32>>>();
        let _ = self.send.send(TorrentManagerMessage::GetHaveMessageProgress { since, reply: tx }).await;
        rx.await.ok().unwrap_or(None)
    }
}