
# Documentation : 

## Learnings/Niche things to note : 

### Tracker protocol : 
- The protocol BEP doesnt mention this, but the charset of the urlencoded querystring needs to be WINDOWS-1252 (latin-1). 
- they serde_qs lib can only do UTF-8 charset. The urlencoded library used by this project gives us the needed querystrings!
