# Kai app icon

## Cobalt orbit — the current icon

The Kai icon is the **cobalt orbit**: a glossy cobalt-blue ribbon (a torus
with a fin) on a pale tile. It is the icon everywhere now: the macOS bundle
icon, the runtime window icon, the Windows executable icon, and the in-app
mark.

The owner-approved master render is `assets/icons/kai-app-v3.png`
(1254 × 1254 RGBA). It was generated from scratch with the built-in image
tool, without reference images; the prompt asked for one continuous cobalt
ribbon loop on a porcelain tile, with no amber, lettering or hand imagery.

## Master and derived assets

Every runtime asset is derived from the master and lives in `assets/icons/`:

- `Kai.iconset/` + `Kai.icns` — the macOS bundle icon, rebuilt from the
  master; `scripts/macos/bundle.sh` copies `Kai.icns` into Kai.app.
- `Kai.ico` — the Windows executable icon: 6 sizes, PNG-compressed entries.
- `kai-app-512.png` — the 512 px full icon; the runtime window icon set via
  `WindowOptions::icon` (X11 and Windows; macOS uses the bundle icon).
- `kai-mark.png` — the ribbon extracted onto transparency at 512 px; the
  in-app mark, drawn by `ui::orb` (home screen, empty states) with an
  orbit-cobalt halo (`brand::ORBIT`), and shown on the splash screen.

## Regeneration

`scripts/icons/build.sh` rebuilds all of the above from the master (default
`assets/icons/kai-app-v3.png`). It needs macOS with `sips`, `iconutil`,
`swiftc` and `python3`. `scripts/icons/extract-mark.swift` is the ribbon
extraction tool — a blueness mask, connected components, and gloss/crevice
handling; the mean core ribbon colour is #3769E3.

## Earlier concepts (kept for comparison)

The first concept was an **amber hand**: a raised hand in warm amber on a
graphite rounded square. It was generated 2026-10-16 and exported as
`assets/icons/kai-app-v1.png` with `Kai-v1.icns` and `Kai-v1.iconset/`.

The second concept was a **folded amber K**: a single sculptural amber ribbon
forming a K on a satin graphite tile. It was exported as
`assets/icons/kai-app-v2.png` and `Kai-v2.icns`; the cobalt orbit superseded
it.

The old SVG hand mark (`assets/icons/kai-hand.svg`) that preceded both was
removed from the tree; nothing references it.

## Verified

- `iconutil -c iconset assets/icons/Kai.icns -o ...` round-trips 10 files.
- `file assets/icons/Kai.ico` reports "MS Windows icon resource - 6 icons".
- The extracted mark was reviewed at 32/48/96/160 px on dark and light.
- The home screen was captured live on macOS, light and dark, and shows the
  new mark; the committed `screenshots/` set (captured on Linux) refreshes
  through the Screenshots workflow on its next run.
