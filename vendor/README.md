# Vendored dependencies

## ely-gpui-component

- **What:** Ely GPUI Components, the UI component library `kai-app` is built
  on.
- **Version:** upstream commit `b0dbfd429c17819f6c143fd365852244d497514a`
  (crate version 0.1.1). This is the same revision `Cargo.toml` pinned before.
- **License:** MIT (`ely-gpui-component/LICENSE`).
- **Upstream:** <https://github.com/ZacharyZhang-NY/Ely-GPUI-Components>.
  Since 2026-10-12 the repository no longer appears to be public, and GitHub
  refuses to clone it. That broke every fresh build and CI run that had no
  cached copy.
- **How it got here:** recovered on 2026-10-12, with the repo owner's
  approval, from CI's cargo cache, which still held cargo's git database for
  the pinned revision. A one-off workflow restored that cache, let cargo check
  the revision out (cargo verifies the commit id), and committed the tree
  without `.git` and `target/` (commit `1657f4d`). The initial recovery did not edit the tree.
- **Workspace:** used as a path dependency and listed in `exclude` in the
  root `Cargo.toml`. It is not a workspace member, so `cargo clippy
  --workspace` and `cargo test --workspace` do not lint or test it.

### Updating

If upstream becomes available again, or a fork is used, change the
`ely-gpui-component` line in the root `Cargo.toml` back to a `git = …, rev =
…` dependency and delete this folder. To change the vendored copy itself,
replace the whole folder with a new upstream revision and record it here.
Avoid local edits, so that the copy stays traceable to upstream.

### Local UI patch — 2026-10-16

`layout/scroll.rs` and `layout/scroll_policy.rs` clamp wheel offsets and pass
boundary events to parent scroll areas. This intentionally differs from the
recovered upstream; preserve or reapply when updating. The workspace app suite
includes the exact pure policy source because this crate is excluded from the
workspace. Live nested scrolling still requires platform interaction checks.

`shell/activity.rs` exposes named, tabbable navigation buttons with focus rings.
The baseline live accessibility tree omitted all six destinations; verify their
names in the bound-window accessibility tree after rebuilding.
