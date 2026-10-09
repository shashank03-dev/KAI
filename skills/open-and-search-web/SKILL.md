---
name: open-and-search-web
description: Open the web browser and search the web for something, then open the best result. Use when the user wants to look something up, find a page, or search online.
license: MIT
metadata:
  author: kai
  version: "1.0"
  kai: '[{"do":"open_url","url":"https://duckduckgo.com"}]'
---
# Open and search the web

Kai has already opened DuckDuckGo in the default browser.

1. Click the search box (it is usually the largest text field near the top).
2. `type` the user's query exactly as they phrased it, then `key` `enter`.
3. Read the results. Click the first result whose title clearly matches what the
   user asked for. Skip ads and "sponsored" results.
4. When the page the user wanted is open, reply `done` with a one-line summary in
   `thought`.

If the user named a specific site (for example "the IEEE CS page on
bmsce.ac.in"), prefer `open_url` straight to that site and look for the link
there instead of searching.
