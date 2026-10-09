---
name: write-email-gmail
description: Compose an email in Gmail in the browser with recipient, subject and body. Use when the user asks to write, draft or send an email or mail someone.
license: MIT
compatibility: Needs the user to be signed in to Gmail in their default browser.
metadata:
  author: kai
  version: "1.0"
  kai: '[{"do":"open_url","url":"https://mail.google.com/mail/u/0/#inbox?compose=new"}]'
---
# Write an email in Gmail

Kai has opened Gmail with a new message window.

1. If a sign-in page shows instead, use `ask_user` to ask the user to sign in,
   then continue. Never type a password yourself.
2. Click the **To** field and type the recipient address.
3. Click **Subject** and type the subject.
4. Click the message body and type the message. Write it in the user's voice,
   short and polite, unless they gave exact wording.
5. Stop before sending. Clicking **Send** is a `high` risk action: only do it if
   the user explicitly asked to send, and always mark it `risk: high` so they
   approve it. Otherwise leave the draft open and reply `done`.
