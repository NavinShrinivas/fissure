use crate::models::torrent_jobs;
use crate::protocols::peer_handshake::PeerConnection;
use byteorder;
use byteorder::BigEndian;
use byteorder::ReadBytesExt;
use crossbeam_channel;
use std::io::Cursor;
use std::io::Read;
use std::time::Duration;
use to_binary;

pub fn state_machine(
    mut conn: PeerConnection,
    unfinished_job_recv: crossbeam_channel::Receiver<torrent_jobs::Job>,
    unfinished_job_snd: crossbeam_channel::Sender<torrent_jobs::Job>,
) {
    let mut data = [0; 4];
    let mut stream = conn.conn;
    let mut pipelined = 0;
    loop {
        if conn.peer_choking == false && pipelined < 5 {
            pipelined += 1;
        }
        if conn.keep_alive.elapsed() > Duration::new(120, 0) {
            // Duration has PartialEq
            stream.shutdown(std::net::Shutdown::Both).unwrap();
            println!("Killing connection");
            return; // Kill the connection
        }
        match stream.read_exact(&mut data) {
            Ok(_) => {
                let msg_len: u32 = u32::from_be_bytes(data);
                // println!("handling handshake response...{}", pstr_len);
                if msg_len == 0 {
                    panic!("Message length should not be 1...");
                } else {
                    let mut data = [0; 1];
                    stream.read_exact(&mut data).unwrap();
                    let id = u8::from_be_bytes(data);
                    match id {
                        0 => {
                            // Choking us
                            conn.peer_choking = true;
                            println!("choking")
                        }
                        1 => {
                            // Unchoking us
                            conn.peer_choking = false;
                            println!("un-choking")
                        }
                        2 => {
                            // Is intrested in what we have (future scope)
                            conn.peer_interested = true;
                            println!("peer_interested")
                        }
                        3 => {
                            // Not intrested
                            conn.peer_interested = false;
                            println!("un peer_interested")
                        }
                        4 => {
                            // Have
                            let data = [0; 1];
                            let zero_based_index =
                                Cursor::new(data).read_u32::<BigEndian>().unwrap() as usize;
                            let value_mut = conn.bitfield.get_mut(zero_based_index).unwrap();
                            *value_mut = 1.to_string();
                            println!("have")
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
                            let data = Vec::new();
                            let mut zero_based_index = String::new();
                            Cursor::new(data)
                                .read_to_string(&mut zero_based_index)
                                .unwrap();
                            for (index, val) in to_binary::BinaryString::from(zero_based_index)
                                .to_string()
                                .chars()
                                .enumerate()
                            {
                                let mut_value = conn.bitfield.get_mut(index).unwrap();
                                *mut_value = val.to_string();
                            }
                            println!("bitfield recvd...")
                        }
                        6 => {
                            // For future expansion, to uploading capabilties at the moment
                            continue;
                        }
                        7 => {}
                        _ => {
                            // println!("Something else {}", id);
                            continue;
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
