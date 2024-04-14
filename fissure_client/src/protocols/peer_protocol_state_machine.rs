use crate::models::torrent_jobs;
use crate::protocols::peer_handshake::PeerConnection;
use byteorder;
use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use crossbeam_channel;
use std::io::Cursor;
use std::io::{Read, Write};
use std::time::Duration;

fn generate_piece_request(job: torrent_jobs::Job) -> String {
    let mut request_str: String = String::new();
    println!(
        "{}{}{}",
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
    let mut stream = conn.conn;
    let mut pipelined = 0;
    println!("Starting protocol state machine");
    loop {
        if pipelined < 5 {
            let job = match unfinished_job_recv.recv() {
                Ok(job) => job,
                Err(e) => {
                    panic!("Not able to recv unfinished job in state machine {}", e);
                }
            };
            println!("[INFO] Trying to pipeline request...{}", job.index);
            if conn.bitfield.get(job.index as usize).unwrap() != "1" {
                // This peer doesnt have the needed piece, hence put back into queue
                let clone_send = unfinished_job_snd.clone();
                tokio::spawn(async move {
                    clone_send.send(job).unwrap(); //MPMC so clone is fine and allowed
                });
            } else if conn.peer_choking == true {
                // We are interested, but we are choked
                // the peer has a piece we want, hence we will send interested request
                // We arent doign anythign smart here as we are putting this piece back into the queue
                // and maybe satisfied by some other peer, but I think is alright
                let mut interested_req = String::new();
                println!(
                    "[INFO] Sending interested request to peer with id {:?}",
                    conn.peer_id
                );
                interested_req = format!("{}{}", interested_req, "0000000102");
                stream
                    .write(hex::decode(interested_req).unwrap().as_slice())
                    .unwrap();
            } else {
                pipelined += 1;
                println!("[INFO] Sending request for piece with index {}", job.index);
                let request_str = generate_piece_request(job); // [TODO] Continue
                println!("[DEBUG] {}", request_str); //[DEBUG]
                stream
                    .write(hex::decode(request_str).unwrap().as_slice())
                    .unwrap();
            }
        }
        if conn.keep_alive.elapsed() > Duration::new(120, 0) {
            // Duration has PartialEq
            stream.shutdown(std::net::Shutdown::Both).unwrap();
            println!("Killing connection");
            return; // Kill the connection if no update for 120 secondss
        }
        let mut data = [0; 4]; // Buffer to find msg len
        match stream.read_exact(&mut data) {
            Ok(_) => {
                let msg_len: u32 = u32::from_be_bytes(data);
                // println!("handling handshake response...{}", pstr_len);
                if msg_len == 0 {
                    panic!("Message length should not be 1...");
                } else {
                    let mut data = [0; 1]; // Buffer to find ID
                    stream.read_exact(&mut data).unwrap();
                    let id = u8::from_be_bytes(data);
                    match id {
                        0 => {
                            // Choking us
                            conn.peer_choking = true;
                            println!("[INFO] getting choking")
                        }
                        1 => {
                            // Unchoking us
                            conn.peer_choking = false;
                            println!("[INFO] getting un-choking")
                        }
                        2 => {
                            // Is intrested in what we have (future scope)
                            conn.peer_interested = true;
                            println!("[INFO] peer_interested in what we have")
                        }
                        3 => {
                            // Not intrested
                            conn.peer_interested = false;
                            println!("[INFO] peer_uninterested")
                        }
                        4 => {
                            // Have
                            let data = [0; 1];
                            let zero_based_index =
                                Cursor::new(data).read_u32::<BigEndian>().unwrap() as usize;
                            let value_mut = conn.bitfield.get_mut(zero_based_index).unwrap();
                            *value_mut = 1.to_string();
                            println!("[INFO] peer telling what it has")
                        }
                        5 => {
                            // Bitfield
                            let rem_len = msg_len - 2;
                            if rem_len * 8 != conn.bitfield.len().try_into().unwrap() {
                                panic!(
                                    "Something wrong, bitfield is not of right size {} {}",
                                    rem_len,
                                    conn.bitfield.len()
                                );
                            }
                            let mut data = vec![0; rem_len as usize]; // Buffer to read bitfield
                            match stream.read_exact(&mut data) {
                                Ok(_) => {
                                    println!("Read the bitfields successfully.");
                                }
                                Err(e) => {
                                    panic!("{}", e);
                                }
                            }
                            let mut binary_flat_map: Vec<char> = Vec::new();
                            for i in data.iter() {
                                let string_rep = format!("{:b}", i);
                                // println!("{}", string_rep); //[DEBUG]
                                for j in string_rep.to_string().chars() {
                                    binary_flat_map.push(j);
                                }
                            }
                            for (index, val) in binary_flat_map.iter().enumerate() {
                                let mut_value = conn.bitfield.get_mut(index).unwrap();
                                *mut_value = val.to_string();
                            }
                            println!("bitfield recvd...")
                        }
                        6 => {
                            // For future expansion, to uploading capabilties at the moment
                            continue;
                        }
                        7 => {
                            println!("We are getting a piece, message incoming SIR! {}", id);
                            //[TODO] Fetch the actual chunk and send it across
                            #[allow(unused)]
                            let zero_based_index =
                                Cursor::new(data).read_u32::<BigEndian>().unwrap() as usize;
                            pipelined -= 1;
                            continue;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                panic!("Failed to receive data: {}", e);
            }
        }
    }
}
