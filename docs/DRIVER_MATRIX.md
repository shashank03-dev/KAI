# Driver regression matrix

Kai acts through the accessibility interface first and falls back to the
mouse when that is not possible. A fallback is silent by design, so a driver
change that breaks the accessibility path would still "work" (more slowly,
moving the pointer, with focus changes) and no task-level benchmark would
notice. The matrix checks the path itself, per toolkit.

## What it does

`crates/kai-os/tests/driver_matrix.rs`, with fixture apps in
`bench/driver/`: a GTK 3 window (`gtk3.py`), a Qt 5 window (`qt5.py`) and a
web page in Firefox (`web.html`). Each fixture has the same three widgets:
an *Apply* button, a *Remember me* checkbox and an empty *Name* field.

For each toolkit and widget the test:

1. reads the real accessibility tree and grounds it, as Kai does;
2. finds the widget by role and name;
3. runs Kai's accessibility action (`kai_os::a11y::act`): press, toggle, or
   fill the empty field;
4. checks the path taken is the expected one (accessibility, or a known
   toolkit limit where Kai must decline);
5. checks the widget really changed, by reading the fixture's **window
   title**, which the fixture sets from its own state (`clicks=1 checked=1
   name=Asha`). The title is an X property, so the check does not trust the
   accessibility tree it is testing.

A fixture that cannot start is reported as skipped, never as passed.

```sh
source scripts/desktop/start.sh
KAI_FIXTURE_PYTHON=python3.12 cargo test -p kai-os --test driver_matrix -- --ignored --nocapture
KAI_MATRIX_TOOLKITS=gtk3 ...   # a subset
```

It needs a desktop, so it is `#[ignore]`d in the normal test run.
`KAI_FIXTURE_PYTHON` names an interpreter with PyGObject and PyQt5 (on this
container `python3` is 3.13 and the distribution's bindings are for 3.12).

## Results (2026-10-12, Ubuntu 24.04 container, Xvfb + openbox + AT-SPI 2.52)

| Toolkit | Press button | Toggle checkbox | Fill empty field |
|---|---|---|---|
| GTK 3.24 | a11y, changed | a11y, changed | a11y, changed |
| Qt 5.15.13 | **known gap** | **known gap** | **known gap** |
| Firefox ESR 140 (web page) | **mouse** (no action exposed) | a11y, changed | Kai declines (mouse), as designed |

- **Qt 5 is a real gap.** The Qt app connects to the accessibility bus but
  publishes no accessible root object (`/org/a11y/atspi/accessible/root`
  does not exist on its bus name) and never appears under the registry. Kai
  sees no tree for Qt 5 apps here and uses the grid. The matrix reports this
  as a known gap, not as a pass, and fails if it ever starts working so the
  expectation gets updated. Not yet known: whether this is the container
  (no session manager, no `QT_QPA_PLATFORMTHEME`) or Qt 5.15 with AT-SPI
  2.52 in general; a KDE desktop would answer it.
- **Firefox page buttons** expose no AT-SPI action at all (measured with an
  `onclick` attribute, an event listener and `type=submit`; checkboxes do
  expose `check`), so Kai presses them with the mouse. Expected `Mouse` in
  the matrix; it fails, and should be updated, if Firefox starts exposing
  `press`.
- **Gecko fills** are declined on purpose: Firefox reports EditableText edits
  on page inputs as done without applying them (`linux_act.rs`,
  `NO_FILL_TOOLKITS`), so Kai types there with the keyboard.

## Guard check

With the button's action names removed from `press_names` (the driver
"silently" losing the press path), the matrix fails:
`gtk3 Apply FAIL took Mouse, expected A11y`. Restored afterwards.

## Not covered yet

Electron/Chromium (no Chromium in the container; Ubuntu's is a snap), Tauri
and WebKitGTK web views, GTK 4, and Windows UI Automation (needs a Windows
machine). Each is one more fixture and one more row.
