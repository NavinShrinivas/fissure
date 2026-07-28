## Fissure : Yet another torrent client, build from scratch (almost)

## Limitation 

### Ones that I inted to solve : 
- ~~Multi folders/multi file downloads - Havent checked if this client works for this case~~ Solved
- Uploads, right now this client only works as a leech.
- Doesnt implement a strong tit for tat economics mechanisim
- Doesnt use DHT as of now. Need to implement DHT and PEX.
- Smarter peer selection - i.e try to download more pieces from peers that are faster.


### Ones that I dont intend to solve : 


## Tasks in focus : 
[ ] Cleaner logging
[ ] Make timings and config applicable amd find the right pieces/peer ratio and timeouts for pieces and blocks to achive better performance
[ ] An UI (Fancy maybe?)
    [ ] wire it up to handle multiple torrents, the underlying services already do.
[ ] Uploads


## Specs I used to build this : 
- The BEP protocol : https://www.bittorrent.org/beps/bep_0000.html
- This is very in depth and very effective : https://wiki.theory.org/BitTorrentSpecification (Note : they don't like people accessing spec page directly, so maybe try browsing from the subdomain root)
- Documentation in this project : [DOCUMENTATION.md](./DOCUMENTATION.md)
- UDP Tracker protcol : https://www.bittorrent.org/beps/bep_0015.html
- Compact tracker response protocol: 
