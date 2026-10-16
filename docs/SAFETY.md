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

- typing into a password or secure field, including typing with no
  target while a password field has keyboard focus (see below);
- payment steps;
- **typing sensitive data**: a card number (Luhn-checked), an IBAN (mod-97
  checked), a national ID number (`123-45-6789`), an API key or token
  (`sk-…`, `ghp_…`, `AKIA…`, or 32+ mixed letters and digits), or a private
  key. Typing it into a form counts as sending it.

Effect words ("send", "delet…", "pay", "order", "submit", …) count where a
word starts with them, also after `re`, `pre`, `un` or `auto` ("resend",
"preorder", "autopay") and inside camel case ("PlaceOrder"), but not
inside other words ("display", "border", "information"). A `wait`, `done`
or `ask_user` never escalates: it cannot send or delete anything.

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

## Typing where focus is

A model reply must name the field it types into (the schema rejects a
`type` with no target). Three paths type into whatever has focus instead: a
skill's fixed `type` step, a cached plan step, and the OpenAI-native loop.
Each reads the accessibility tree first, where the OS reports focus (AT-SPI
`Focused`, UI Automation `HasKeyboardFocus`), and:

- a focused password or secure field means the step is not run blind: a
  skill or cached step hands over to the model (whose `type` must then name
  the field and is approved like any password step), the native loop stops
  and asks you;
- no focused element reported but a password field on screen counts as the
  password field (Kai cannot rule it out);
- a CAPTCHA, code request or suspicious text on that screen also hands over.

A replayed procedure step (`docs/PROCEDURES.md`) reads the screen before
every step, untargeted ones too, so the same checks run there.

## Approvals in Team mode

Sub-agents can ask at the same time. Approvals and human checks queue per
run and show one at a time; one sub-agent acting or finishing its check
never drops another's (a dropped approval answers Stop). Stop answers every
queued one. An Approve within 600 ms of an approval appearing is ignored, so
a double click cannot approve the queued step that takes the first one's
place unread.

## Kai does not stop itself

A shell step that would kill Kai is not run: a kill-family command (`kill`,
`pkill`, `killall`, `taskkill`, `Stop-Process`) naming Kai's process or its
parent, `$PPID`, every process (`kill -9 -1`), or Kai by name, including a
pipeline that feeds it (`pgrep kai | xargs kill`). Only the statement that
holds the killer counts. Not caught: a kill by user (`pkill -u $USER`) or
by a PID computed at run time (`kill $(cat pidfile)`).

## Tests

| Guarantee | Test |
|---|---|
| An injection on screen pauses before any action; headless runs stop | `safety_e2e::an_injection_on_screen_pauses_before_anything_runs` |
| The user decides; continuing tells the model screen text is data; Stop stops | `safety_e2e::the_user_decides_and_the_model_is_told_screen_text_is_data` |
| A card number needs approval even on autopilot; trace and later turns never hold it; the explanation names action, risk and use | `safety_e2e::a_card_number_is_confirmed_even_on_autopilot_and_never_saved` |
| A skill step never types blind into a focused password field (and still types into an ordinary one) | `safety_focus_e2e::a_skill_step_never_types_blind_into_the_focused_password_field` |
| A replay reads the screen before an untargeted step; an injection there stops it before anything runs | `safety_focus_e2e::a_replay_reads_the_screen_before_an_untargeted_first_step` |
| Untargeted typing follows focus; unknown focus with a password field on screen asks; any focused secure field counts | `risk::tests::untargeted_typing_*`, `risk::tests::any_focused_secure_field_counts_when_several_report_focus` |
| Effect words escalate after a prefix and in camel case, not inside other words | `risk::tests::effect_words_*` |
| Team approvals queue, are never dropped by another sub-agent, and a fast Approve is ignored | `state::tests` (kai-app) |
| Commands that would kill Kai are refused; ordinary kills run | `desktop::tests::a_command_that_would_kill_kai_is_refused`, `desktop::tests::the_kill_guard_sees_pipelines_paths_and_signals` |
| What counts as sensitive, and what does not (emails, dates, URLs, order numbers) | `untrusted::tests` |
| Ordinary screens ("Password", "Verify your email address", "System settings") are not flagged | `untrusted::tests::ordinary_screens_are_not_flagged` |

## Limits

- The phrase lists catch known wording, not every possible injection. The
  prompt rule and the approval rules cover the rest: an injected
  instruction to send, buy, delete or type a secret still has to pass your
  approval.
- A screen with no accessibility tree (a grid) has no text to scan, so only
  the prompt rule applies there. It has no focus either: an untargeted
  skill `type` on a grid screen is not checked for a password field.
- Focus is read just before the step; an app that moves focus between that
  read and the keystrokes is not caught.
- Phrases match whole words ("to any airline" is not "to any AI"), but a
  legitimate page can still use one ("do not close this window" during a
  payment). Kai then asks once; continuing is one click.
- Screenshots sent to the model, and the frames saved when recording is on,
  show the screen as it is: a field that displays a typed card number shows
  it there.

## Settings durability (2026-10-16 design note)

The review found that an interrupted in-place write could truncate
`settings.json`, after which a default settings object could overwrite the
damaged file. Save a complete temporary file in the same directory, sync it,
then rename it over the old one. Refuse to save while an existing file is
unreadable, so its contents remain available for repair. Keep storage code in
`settings_store.rs` to preserve the repository's 500-line file limit. An
in-place write was rejected because it destroys the only copy on failure;
silently backing up and replacing a corrupt file was rejected because it
hides the problem. The focused test writes valid settings, corrupts the file,
then proves the next save refuses to replace it. The app already shows a
settings-save error toast, so this failure reaches the user.
