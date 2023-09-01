
# Documentation : 

## Learnings/Niche things to note : 

### Tracker protocol : 
- The protocol BEP doesnt mention this, but the charset of the urlencoded querystring needs to be WINDOWS-1252 (latin-1). 
- they serde_qs lib can only do UTF-8 charset. The urlencoded library used by this project gives us the needed querystrings!


### Peer protocols : 
- The bitfield message may only be sent immediately after the handshaking sequence is completed, and before any other messages are sent. It is optional, and need not be sent if a client has no pieces. The bitfield message is variable length, where X is the length of the bitfield. The payload is a bitfield representing the pieces that have been successfully downloaded. The high bit in the first byte corresponds to piece index 0. Bits that are cleared indicated a missing piece, and set bits indicate a valid and available piece. Spare bits at the end are set to zero. Some clients (Deluge for example) send bitfield with missing pieces even if it has all data. Then it sends rest of pieces as have messages. They are saying this helps against ISP filtering of BitTorrent protocol. It is called lazy bitfield. A bitfield of the wrong length is considered an error. Clients should drop the connection if they receive bitfields that are not of the correct size, or if the bitfield has any of the spare bits set.

- The handshake goes two ways and is not covered in the protocol, this code base implements it as part of the protocols/peer_handshake.rs file
