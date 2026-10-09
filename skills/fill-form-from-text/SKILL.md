---
name: fill-form-from-text
description: Fill in a web or desktop form using details the user pasted or described, such as name, email, phone and address. Use when the user asks to fill, complete or enter details into a form.
license: MIT
metadata:
  author: kai
  version: "1.0"
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
4. Do not press **Submit**, **Send** or **Pay**. Those are `high` risk. Reply
   `done` once every field you have a value for is filled, and say which fields
   you left empty.
