---
name: fill-form-from-text
description: Fill in a web or desktop form using details the user pasted or described, such as name, email, phone and address. Use when the user asks to fill, complete or enter details into a form.
license: MIT
metadata:
  author: kai
  version: "1.1"
---
# Fill a form from text

1. Read the user's text and pull out each value (name, email, phone, address,
   dates). Do not invent values that are not there; leave those fields empty.
2. Work top to bottom through the form's fields in the element list. For each
   text field, `type` with its mark as `target`. For dropdowns, click them and
   then click the matching option. For checkboxes, click only if the text says
   so.
3. Password and payment fields: never fill them. Use `ask_user` and let the user
   type those themselves.
4. If the user asked you to send or submit the form, click **Send** or
   **Submit** yourself once every field is filled, with `risk` set to
   `high`. Do not use `ask_user` for this: the request was the user's
   permission, and Kai shows its own approval prompt for high-risk clicks.
   Never press **Pay** yourself. If the user did not ask you to send it,
   don't: reply `done` once every field you have a value for is filled, and
   say which fields you left empty.
