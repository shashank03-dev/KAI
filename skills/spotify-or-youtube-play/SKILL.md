---
name: spotify-or-youtube-play
description: Play a song, artist, album, playlist or video on YouTube or Spotify. Use when the user asks to play music, a song or a video.
license: MIT
metadata:
  author: kai
  version: "1.0"
---
# Play music or a video

1. Pick the service: Spotify if the user said Spotify, otherwise YouTube.
2. Go straight to the search results with `open_url` instead of clicking
   through the home page:
   - YouTube: `https://www.youtube.com/results?search_query=<words joined with +>`
   - Spotify: `https://open.spotify.com/search/<words, spaces as %20>`
3. Click the first result that matches the title and artist the user asked for.
   On YouTube, skip results marked "Ad" and shorts unless asked.
4. If it does not start by itself, click the play button.
5. Reply `done` once it is playing.
