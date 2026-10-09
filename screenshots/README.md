# Screenshots

Captured with the app's own harness on a real desktop:

```sh
KAI_CAPTURE=screenshots cargo run --release -p kai-app
```

It writes `NN-<screen>-<light|dark>.png` for all 21 screens and states and
quits. See DECISIONS.md for why they could not be captured in the headless
build container.
