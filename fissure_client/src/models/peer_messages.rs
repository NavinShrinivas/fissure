use crate::bee_processor::bee_decoder::FissureErr;
use bitvec::{order::Msb0, vec::BitVec};
use bytes::{BytesMut, BufMut};
use tokio_util::codec::{Decoder, Encoder};


pub enum PeerMessage{
    KeepAlive,
    Choke, //we are getting choked
    Unchoke, //we are getting unchoked
    Interested, //Peer is interested in what we have
    NotInterested, //Peer is not interested in what we have
    Have{piece_index: u64},
    Bitfield{bitfield: BitVec<u8, Msb0>},
    Request{piece_index: u64, block_offset: u64, length: u64}, 
    //TODO - We should see how to handle above request,
    // AKA how to reply to them from the writer loop
    Piece {piece_index: u64, block_offset: u64, data: Vec<u8>},
    #[allow(dead_code)]
    Cancel {piece_index: u64, block_offset: u64, length: u64}, 
    //Cancel cancels any request the peer may have made, used during endgames specifically
    Port {
        //TODO -- used by newer DHT protocol
    }
}

impl PeerMessage {
    pub fn decode(data: &[u8]) -> Self {
        // If the slice passed here is empty, it means we received a length-prefix of 0
        if data.is_empty() {
            return PeerMessage::KeepAlive;
        }

        // The first byte of the data payload is always our Message Type ID
        let message_id = data[0];

        match message_id {
            0 => PeerMessage::Choke,
            1 => PeerMessage::Unchoke,
            2 => PeerMessage::Interested,
            3 => PeerMessage::NotInterested,
            4 => {
                // Have: 4-byte big-endian integer for piece index
                let piece_index = u32::from_be_bytes(data[1..5].try_into().unwrap()) as u64;
                PeerMessage::Have { piece_index }
            }
            5 => {
                // Bitfield: The rest of the payload is the raw bit map slice
                let bitfield_bytes = data[1..].to_vec();
                let bitfield = BitVec::from_vec(bitfield_bytes);
                PeerMessage::Bitfield { bitfield }
            }
            6 => {
                // Request: index (4), begin (4), length (4)
                let piece_index = u32::from_be_bytes(data[1..5].try_into().unwrap()) as u64;
                let block_offset = u32::from_be_bytes(data[5..9].try_into().unwrap()) as u64;
                let length = u32::from_be_bytes(data[9..13].try_into().unwrap()) as u64;
                PeerMessage::Request { piece_index, block_offset, length }
            }
            7 => {
                // Piece: index (4), begin (4), block data (variable length)
                let piece_index = u32::from_be_bytes(data[1..5].try_into().unwrap()) as u64;
                let block_offset = u32::from_be_bytes(data[5..9].try_into().unwrap()) as u64;
                let data = data[9..].to_vec(); // The remaining bytes are the block itself
                PeerMessage::Piece { piece_index, block_offset, data }
            }
            8 => {
                // Cancel: index (4), begin (4), length (4)
                let piece_index = u32::from_be_bytes(data[1..5].try_into().unwrap()) as u64;
                let block_offset = u32::from_be_bytes(data[5..9].try_into().unwrap()) as u64;
                let length = u32::from_be_bytes(data[9..13].try_into().unwrap()) as u64;
                PeerMessage::Cancel { piece_index, block_offset, length }
            }
            _ => {
                // Default fallback for newer protocols or DHT extension messages
                PeerMessage::Port {}
            }
        }
    }
}

pub struct PeerCodec{}

impl PeerCodec{
    pub fn new() -> Self{
        PeerCodec {  }
    }
}

impl Decoder for PeerCodec{
    type Item = PeerMessage;

    type Error = FissureErr;

    fn decode(&mut self, src: &mut tokio_util::bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4{
            return Ok(None)
        }
        let msg_len = u32::from_be_bytes(src[0..4].try_into().unwrap()) as usize;

        if src.len() < 4 + msg_len {
            //We havent recived the full message in buffer yet
            //We will wait for the full message to come before we remove it out of the 
            //buffer:
            return Ok(None)

        }
        let full_data = src.split_to(4 + msg_len);
        Ok(Some(PeerMessage::decode(&full_data[4..]))) //decode excluding the length
    }
}

impl Encoder<PeerMessage> for PeerCodec {
    type Error = FissureErr;

    fn encode(&mut self, item: PeerMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match item {
            PeerMessage::Interested => {
                // Total message size = 1 byte (the ID)
                dst.put_u32(1); // 4-byte Message Length
                dst.put_u8(2);  // 1-byte Message Type ID (2 = Interested)
            }
            PeerMessage::Request { piece_index, block_offset, length } => {
                // Total message size = 1 byte (ID) + 4 (index) + 4 (offset) + 4 (length) = 13 bytes
                dst.put_u32(13); // 4-byte Message Length
                dst.put_u8(6);   // 1-byte Message Type ID (6 = Request)
                
                // BitTorrent wire protocol demands Big-Endian integers
                dst.put_u32(piece_index as u32);
                dst.put_u32(block_offset as u32);
                dst.put_u32(length as u32);
            }
            PeerMessage::KeepAlive => {
                // Keep alive is explicitly a 4-byte length of 0 with no ID
                dst.put_u32(0);
            }
            _ => {
                // For v1, we can stub out or log other outbound messages we don't send yet
                log::warn!("Outbound message serialization not implemented for this variant.");
            }
        }
        Ok(())
    }
}