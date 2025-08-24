use crate::models::torrent_jobs;
use crate::models::torrent_jobs::Job;
use crate::protocols::peer_handshake::PeerConnection;
use byteorder;
use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use crossbeam_channel;
use std::collections::HashMap;
use std::io::Cursor;
use std::io::{Read, Write};
use std::time::Duration;

use log::{debug, error, info};

fn generate_piece_request(job: &torrent_jobs::Job) -> String {
    let mut request_str: String = String::new();
    info!(
        "request piece info : {} {} {}",
        job.index.to_string(),
        job.begin.to_string(),
        job.length.to_string()
    );
    // All the numbers in the request message are 4 byte hex reps, implying 8 hex chars, 4 sets
    request_str = format!("{}{}", request_str, "0000000D06");
    request_str = format!("{}{}", request_str, format!("{:08X}", job.index));
    request_str = format!("{}{}", request_str, format!("{:08X}", job.begin));
    request_str = format!("{}{}", request_str, format!("{:08X}", job.length));
    return request_str;
}

// To represent 1 hex char you need 4 bit, 2 hex chars is one byte, 4 bytes is 8 hex chars

pub fn state_machine(
    mut conn: PeerConnection,
    unfinished_job_recv: crossbeam_channel::Receiver<torrent_jobs::Job>,
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
) {
    let mut stream= conn.conn;
    let mut pipelined = 0;
    let mut pipelined_tasks: HashMap<String, Job> = HashMap::new();
    debug!("Starting protocol state machine");
    loop {
        if pipelined < 5 {
            let job = match unfinished_job_recv.recv() {
                Ok(job) => job,
                Err(e) => {
                    panic!("Not able to recv unfinished job in state machine {}", e);
                }
            };
            debug!(
                "[INFO] Trying to pipeline request...{} {:?}",
                job.index, conn.peer_id
            );
            if conn.bitfield.get(job.index as usize).unwrap() != "1" {
                // This peer doesnt have the needed piece, hence put back into queue
                let clone_send = unfinished_job_snd.clone();
                tokio::spawn(async move {
                    clone_send.send(job).unwrap(); //MPMC so clone is fine and allowed
                });
            } else if conn.peer_choking == true {
                // We are interested, but we are choked
                // the peer has a piece we want, hence we will send interested request
                // We arent doing anything smart here as we are putting this piece back into the queue
                // and maybe satisfied by some other peer, but I think is alright
                let mut interested_req = String::new();
                debug!(
                    "[INFO] Sending interested request to peer with id {:?}",
                    conn.peer_id
                );
                interested_req = format!("{}{}", interested_req, "0000000102");
                stream
                    .write(hex::decode(interested_req).unwrap().as_slice())
                    .unwrap();
            } else {
                pipelined += 1;
                debug!("[INFO] Sending request for piece with index {}", job.index);
                let request_str = generate_piece_request(&job); // [TODO] Continue
                debug!("[DEBUG] {}", request_str); //[DEBUG]
                let piece_id = format!("{}{}{}", job.index, job.begin, job.length);
                pipelined_tasks.insert(piece_id, job);
                stream
                    .write_all(hex::decode(request_str).unwrap().as_slice())
                    .unwrap();
            }
        }
        if conn.keep_alive.elapsed() > Duration::new(120, 0) {
            // Duration has PartialEq
            stream.shutdown(std::net::Shutdown::Both).unwrap();
            info!("Dead connection. Killing connection : {:?}", conn.peer_id);
            return; // Kill the connection if no update for 120 secondss
        }
        let mut data: [u8; 4] = [0; 4]; // Buffer to find msg len
        match stream.read(&mut data) {
            Ok(n) => {
                if n==0 {
                    continue;
                }
                let msg_len: u32 = u32::from_be_bytes(data);
                if msg_len == 0 {
                    error!("Message length should not be 0...");
                    continue;
                } else {
                    let mut msp_type = [0; 1]; // Buffer to find ID
                    stream.read_exact(&mut msp_type).unwrap();
                    let id = u8::from_be_bytes(msp_type);
                    let mut remaining_data = vec![0; msg_len as usize - 1 as usize];
                    match stream.read_exact(&mut remaining_data){
                        Ok(()) => {
                            debug!("Read {} bytes after message type in new message", msg_len - 1);
                        }, 
                        Err(e) => {
                            //return jobs in pipeline back to unfinished queue
                            let clone_send = unfinished_job_snd.clone();
                            tokio::spawn(async move {
                                let keys: Vec<String> = pipelined_tasks.keys().cloned().collect();
                                for k in keys {
                                    let u_job = pipelined_tasks.remove(&k).unwrap();
                                    clone_send.send(u_job).unwrap(); //MPMC so clone is fine and allowed
                                }
                            });
                            error!("Ran into an error reading remaining response body : {}. Shutting down connection with this peer.", e);
                            stream.shutdown(std::net::Shutdown::Both).unwrap();
                            return;

                        }

                    };
                    match id {
                        0 => {
                            // Choking us
                            conn.peer_choking = true;
                            debug!("[INFO] getting choking")
                        }
                        1 => {
                            // Unchoking us
                            conn.peer_choking = false;
                            debug!("[INFO] getting un-choking")
                        }
                        2 => {
                            // Is interested in what we have (future scope)
                            conn.peer_interested = true;
                            debug!("[INFO] peer_interested in what we have")
                        }
                        3 => {
                            // Not interested
                            conn.peer_interested = false;
                            debug!("[INFO] peer_uninterested")
                        }
                        4 => {
                            // Have
                            info!("[INFO] peer telling what it has");
                            let arr: [u8; 4] = remaining_data[..4].try_into().expect("slice with incorrect length");
                            let value_mut = conn.bitfield.get_mut(u32::from_be_bytes(arr) as usize).unwrap();

                            *value_mut = 1.to_string();
                        }
                        5 => {
                            // Bitfield
                            let bitfield_size: usize = conn.bitfield.len() / 8;
                            let bitfield_data : Vec<u8> = remaining_data[..bitfield_size].to_vec();// Buffer to read bitfield
 
                            let mut binary_flat_map: Vec<char> = Vec::new();
                            for i in bitfield_data.iter() {
                                let string_rep = format!("{:b}", i);
                                for j in string_rep.to_string().chars() {
                                    binary_flat_map.push(j);
                                }
                            }
                            for (index, val) in binary_flat_map.iter().enumerate() {
                                let mut_value = conn.bitfield.get_mut(index).unwrap();
                                *mut_value = val.to_string();
                            }
                            debug!("bitfield recvd...")
                        }
                        6 => {
                            //UPLOADING
                            // For future expansion, to uploading capabilties at the moment
                            continue;
                        }
                        7 => {
                            //PIECE
                            info!("We are getting a piece, {}", id);

                            let piece_index_bin: [u8; 4] = remaining_data[..4].try_into().expect("slice with incorrect length for piece index");
                            let piece_idx = u32::from_be_bytes(piece_index_bin);

                            let chunk_offset_bin: [u8; 4] = remaining_data[4..8].try_into().expect("slice with incorrect length for piece index");
                            let chunk_offset = u32::from_be_bytes(chunk_offset_bin);


                            let chunk_data = remaining_data[8..].to_vec();

                            //[TODO] Hash check

                            let piece_id =
                                format!("{}{}{}", piece_idx, chunk_offset, chunk_data.len());

                            if pipelined_tasks.contains_key(piece_id.as_str()){
                                info!("Piplined task over!");
                                pipelined_tasks.remove(&piece_id);
                            }else{
                                info!("Random piece");
                            }
                            info!("{} {} {}", piece_idx, chunk_offset, chunk_data.len());

                            pipelined -= 1;
                            continue;
                        }
                        _ => {
                            error!("[CRITICAL] We are seeing something wrong wrt to message id. Possibly missed reading offset from TCP Stream buffers.")
                        }
                    }
                }
            }
            Err(e) => {
                panic!("Failed to receive data: {}", e);
            }
        }
    }
}
