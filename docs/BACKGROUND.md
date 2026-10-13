# Background, window-scoped driving: what was built and what was not

Item 3 of the 2026-10-12 handoff (`docs/RESEARCH_2026_10_FOUNDATIONS.md` §4)
asked for three things: a pointer fallback that does not steal focus or move
the real cursor, per-window capture, and a session per sub-agent so *Team*
screen subtasks run side by side. Research came first; only the part that can
be built and tested honestly on Linux was built.

## What the platforms allow (researched 2026-10-12)

**Input without the real pointer.**

- *Linux, X11.* `XSendEvent` delivers synthetic events with the
  `send_event` flag set, and GTK and Qt ignore those (xdotool's man page:
  input to a specific window "is likely ignored"; forum reports for GTK, Qt
  and Wine). XTEST (what xdotool and enigo use without a window id) goes to
  whatever is under the real pointer, so it moves the cursor. AT-SPI's
  `GenerateMouseEvent` is implemented with XTEST too. There is no pointer
  path on X11 that both works in GTK/Qt and leaves the cursor alone.
- *Linux, Wayland.* Clients cannot inject input into other clients at all
  (by design); only the compositor, or a portal session the user approves,
  can.
- *Windows.* `PostMessage(WM_LBUTTONDOWN/UP)` to a window handle needs no
  focus and no cursor move and works for classic Win32 controls, but
  Chromium, WPF and UWP do their own hit testing and raw input and ignore
  it. `SendInput` moves the real cursor. This cannot be compiled or tested in
  this container (the Windows cross-check stops at `ring`).

So the background path that works everywhere is the one Kai already has:
**accessibility actions** (AT-SPI Action / EditableText, UI Automation
patterns), which need neither focus nor the pointer. The driver matrix
(`docs/DRIVER_MATRIX.md`) now checks that path per toolkit.

**Per-window capture.**

- *Windows.* `PrintWindow` with the undocumented `PW_RENDERFULLCONTENT`
  flag (2) captures a covered window, including DirectComposition content,
  from Windows 8.1 (WebRTC and Chromium use it). xcap 0.9, which Kai already
  uses, calls exactly that for `Window::capture_image`.
- *Linux, X11.* `XGetImage` on a window returns garbage for covered parts
  unless a compositor keeps an off-screen buffer; the container has none.
- *Linux, Wayland.* Per-window capture goes through the screencast portal
  and asks the user.

## What was built: window-scoped frames

`KAI_WINDOW_SCOPE=1` (off by default, `crates/kai-os/src/window.rs`). The
model sees the focused app instead of the whole display:

- the region is the focused window plus every other visible window of the
  same process (on X11 and Windows a menu, popup or dialog is its own
  top-level window), clamped to the display;
- it is cut out of the normal display capture, so the coordinates, the
  masking of Kai's own windows and the click path stay exactly as they are;
- tree-only steps use the same region, so mark numbers agree between full
  and tree-only observations; marks outside the region are dropped;
- Kai's own window having focus, no focused window (Wayland lists none), or
  a region under 32 px falls back to the whole display.

Why this and not true background capture: it is the part that helps on
every platform today. Other apps (a chat, a mail window) never reach the
model, which matters for privacy and for injected text, and a small window
is sent at its own size instead of being shrunk with the whole screen, so it
costs fewer image tokens and keeps text legible. Checked live: with pcmanfm
focused and Mousepad open beside it, the frame was pcmanfm's 640x480 window
with its marks in place, Mousepad absent.

Not done: capturing a covered window. On Windows the xcap call is there
(`Window::capture_image`) but untested here; on X11 it would need a
compositor.

## What was not built, and why

- **A pointer fallback that does not move the cursor.** None exists on X11
  or Wayland for GTK/Qt (above). Restoring the cursor afterwards would still
  move it and still raise the window. On Windows `PostMessage` would help
  some apps but could not be built or tested here; it belongs in a Windows
  session with a real machine.
- **A session per sub-agent.** Running screen subtasks side by side needs
  each one to act without focus or the pointer. With only accessibility
  actions being focus-free, two GUI subtasks would still fight over the
  keyboard focus and the pointer whenever either falls back to the mouse,
  which is common (`a11y` covers press and empty-field fill only). Team mode
  keeps taking turns for screen subtasks until a focus-free fallback exists.

## How to measure

`KAI_WINDOW_SCOPE=1 kai bench --backend gemini --model gemma-4-26b-a4b-it`
against the same run without it; compare pass rate, steps and tokens in
(image tokens fall when windows are smaller than the screen). Results:
`bench/results/2026-10-12-live.md`.
