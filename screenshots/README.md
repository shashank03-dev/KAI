# Screenshots

Captured with the app's own harness (made-up run data, no model):

```sh
KAI_CAPTURE=screenshots cargo run --release -p kai-app
```

It writes `NN-<screen>-<light|dark>.png` for all 25 screens and states and
quits. These were captured in the headless build container on TigerVNC's
`Xvnc` (1600x1000x24) with Mesa's llvmpipe Vulkan driver (lavapipe), which
gpui's wgpu renderer picked by itself. The sample frame in the run views is
`docs/img/set-of-marks-gtk.jpg` (its black area is part of that capture).

Why earlier attempts were black: kai-app built `gpui_platform` without its
`x11`/`wayland` features, so gpui-pre-linux ran its headless-only platform and
never opened an X window (`xwininfo -root -tree` showed no children). Turning
the features on in `crates/kai-app/Cargo.toml` fixed both the screenshots and
the app itself on Linux.
