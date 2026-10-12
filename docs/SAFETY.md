# Safety: who Kai listens to, and when it asks

## Only the user's words are instructions

The task you give Kai, and the messages you send while it runs, are
instructions. Everything Kai sees is **data**: web pages, PDFs, emails,
calendar invites, chats, notifications, command output, and anything else on
screen. That holds even when the text looks urgent, claims to come from you
or an administrator, or claims to override Kai's rules.

Three layers:

1. **The system prompt** says so, and tells the model to use `ask_user` when
   it sees phishing, spam, text addressed to an AI, or an unexpected warning.
2. **A check in code** (`crates/kai-core/src/untrusted.rs`) reads every
   screen before the model sees it. Text that matches a prompt-injection
   phrase ("ignore previous instructions", "AI agent:", "do not tell the
   user", …) or a phishing or scareware phrase ("your computer is infected",
   "call Microsoft support", "your account will be suspended", …) pauses the
   run, like a CAPTCHA:

   > Kai found something suspicious on screen. The screen shows what looks
   > like a prompt injection (text telling an AI what to do): "…". Kai never
   > follows instructions from the screen. Continue the task and ignore it,
   > or stop?

   **Ignore it and continue** goes on and tells the model to treat that text
   as data. **Stop** ends the run. Headless runs (`kai bench`, piped input)
   stop as "asked user". The same text is asked about once per run.
3. **Approvals and guards never read screen text as permission.** Risk is
   classified from what an action does, and the approval comes from you.

`kai mcp` flags the same content in `kai_observe`.

## Ask at the point of risk, not before

Kai starts tasks without asking when the next step is safe. It asks right
before the risky step: sending, buying, deleting, submitting, changing
system settings (on the default approval level), and **always** for these,
whatever the level, autopilot included:

- typing into a password or secure field;
- payment steps;
- **typing sensitive data**: a card number (Luhn-checked), an IBAN (mod-97
  checked), a national ID number (`123-45-6789`), an API key or token
  (`sk-…`, `ghp_…`, `AKIA…`, or 32+ mixed letters and digits), or a private
  key. Typing it into a form counts as sending it.

For sensitive data the approval prompt explains the action, the risk and how
the data is used, for example:

> Kai wants to type your card number into the textfield "Card number". Risk:
> typing it sends it to whatever app or site owns that field. It is typed
> there once and nowhere else; Kai's lessons, procedures and traces blank it
> out.

That is true by construction: the saved trace (the step and the raw model
reply it came from) and the model's later turns show `[card number]`. A
field that shows the typed value again on a later screen is blanked to
`[sensitive data]` before the model, the trace or a lesson reads it. A step
that needed approval never becomes a procedure, and lessons are learned from
the redacted trace.

## Tests

| Guarantee | Test |
|---|---|
| An injection on screen pauses before any action; headless runs stop | `safety_e2e::an_injection_on_screen_pauses_before_anything_runs` |
| The user decides; continuing tells the model screen text is data; Stop stops | `safety_e2e::the_user_decides_and_the_model_is_told_screen_text_is_data` |
| A card number needs approval even on autopilot; trace and later turns never hold it; the explanation names action, risk and use | `safety_e2e::a_card_number_is_confirmed_even_on_autopilot_and_never_saved` |
| What counts as sensitive, and what does not (emails, dates, URLs, order numbers) | `untrusted::tests` |
| Ordinary screens ("Password", "Verify your email address", "System settings") are not flagged | `untrusted::tests::ordinary_screens_are_not_flagged` |

## Limits

- The phrase lists catch known wording, not every possible injection. The
  prompt rule and the approval rules cover the rest: an injected
  instruction to send, buy, delete or type a secret still has to pass your
  approval.
- A screen with no accessibility tree (a grid) has no text to scan, so only
  the prompt rule applies there.
- Phrases match whole words ("to any airline" is not "to any AI"), but a
  legitimate page can still use one ("do not close this window" during a
  payment). Kai then asks once; continuing is one click.
- Screenshots sent to the model, and the frames saved when recording is on,
  show the screen as it is: a field that displays a typed card number shows
  it there.
