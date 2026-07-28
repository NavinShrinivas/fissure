## Fissure : Yet another torrent client, build from scratch (almost)

## Limitation 

### Ones that I inted to solve : 
[x] Multi folders/multi file downloads - Havent checked if this client works for this case
[ ] Uploads, right now this client only works as a leech.
[ ] Doesnt implement a strong tit for tat economics mechanisim
[ ] Doesnt use DHT as of now. Need to implement DHT and PEX.

### Ones that I dont intend to solve : 


## Tasks in focus : 
[ ] Cleaner logging
[ ] An UI (Fancy maybe?)
[ ] Uploads


## Specs I used to build this : 
- The BEP protocol : https://www.bittorrent.org/beps/bep_0000.html
- This is very in depth and very effective : https://wiki.theory.org/BitTorrentSpecification (Note : they don't like people accessing spec page directly, so maybe try browsing from the subdomain root)
- Documentation in this project : [DOCUMENTATION.md](./DOCUMENTATION.md)
- UDP Tracker protcol : https://www.bittorrent.org/beps/bep_0015.html
- Compact tracker response protocol: 
