## Fissure : A toolset for bittorrent

### Specs : 
- The BEP protocol : https://www.bittorrent.org/beps/bep_0000.html
- This is very in depth and very effective : https://wiki.theory.org/BitTorrentSpecification (Note : they don't like people accessing spec page directly, so maybe try browsing from the subdomain root)
- Documentation in this project : [DOCUMENTATION.md](./DOCUMENTATION.md)
### SDLC : 
- A highy iterative approach 

### Status (going downward, kinda like a road map): 
- Inital project is the torrent client, may create the tracker server if we get the client running well :)
- Far from completion, WIP. Tracker protocols are complete. Peer protocols are next up.
- peer hanshake is done next up : 
    - tcp time outs 
    - error handling (deperately needed)
    - selective logging (very much needed)
    - the crux of this project : THE peer protocol
    - Cleaning up of dependencies, they are really bloated.
