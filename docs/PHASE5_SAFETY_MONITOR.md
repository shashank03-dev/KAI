# Phase 5: local computer safety checks

Design note (2026-10-16). The implementation follows
`RESEARCH_2026_10_OPENAI_PARITY.md` §§2.4, 5f and 7. The research collected
the current OpenAI/Azure protocols and Operator monitor results on that date.

The local harness raises `malicious_instructions`, `irrelevant_domain` and
`sensitive_domain` before computer actions. URLs use the WHATWG URL parser and
a bundled public-suffix database (the `psl` crate) to avoid confusing tenants
on private suffixes such as `github.io`. A domain is allowed if named in the
task, explicitly approved this run, or reached by up to two observed link-click
hops from such a domain. Unknown or malformed addresses never grant access.
Domain parsing, allowlists and bounded hop state live in `safety_domains.rs`;
action checks live in `safety_checks.rs`. A clicked link grants a hop only once
the observed site changes, and any intervening non-link action revokes that
pending grant. Malformed HTTP(S) origins cannot be remembered as trusted:
each attempted action requires fresh human review, while invalid `open_url`
requests are rejected by the computer action validator.
Users can preapprove individual sites in an optional
`<settings>/allowed_domains.txt` (one site per line). Public suffixes alone
and malformed entries are ignored; private suffix tenants remain distinct.

The optional screenshot monitor runs only on pages containing imperative text
that the existing injection detector did not flag. It sees the screenshot and
a bounded excerpt of untrusted screen/page text; it returns a strict JSON
boolean. Positive, malformed and failed answers pause for user review before
the action. The monitor never executes instructions from that text. We reject
continuous model monitoring because of cost, false positives and sensitive
data exposure; there is no monitor call without a candidate or on pages
containing secure fields. A tree-only candidate gets a fresh screenshot;
if it cannot be verified, the action pauses for review.

Sensitive pages require a safety acknowledgement and a fresh per-action
human presence check until the run leaves them, including observe, move and
wait actions. Offline and unattended agents
must not auto-acknowledge; the UI's `approve_safety_check` controls this.

`kai run --injection-monitor MODEL` and `kai bench --injection-monitor
MODEL` opt in to a second model client on the selected backend. The switch is
independent from `--local-safety-checks`, though both checks use the same
local approval gate. Browser page text and accessibility text are considered;
tree-only observations capture a fresh image before a monitor call. Monitor
answers must be exact JSON with one boolean `malicious` key. A missing,
stale, erroneous or malformed screenshot/verdict forces review. `--yes`
cannot auto-approve monitor findings or accept these safety checks in a
headless terminal. Monitor timeouts cover page reads,
screenshot capture and model classification, and the run's cancellation switch
can interrupt an in-flight monitor call.
Native OpenAI computer calls can opt into the **rule-only** local path with
`--local-safety-checks` (`CuaConfig.local_safety_checks`, default off). Each
queued native action receives a fresh rule check before dispatch, merged by
code with any provider checks for one human decision. Only original provider
check IDs are carried to the Responses API; local `kai_sc_*` IDs stay local.
The native path evaluates domain, sensitive-site/watch and target-text
injection rules. Its single-agent `--injection-monitor MODEL` option also
uses the shared screenshot/LLM injection monitor through
`CuaRunner::run_with_monitor`, separately opt-in from local rules. Verdicts,
model errors and cancellation follow the same fail-closed human-review gate
before action. Multi-agent runs still reject `--injection-monitor`.

Native link-hop tracking now calls the shared `RunDomains` logic. A successful
coordinate click may grant an observed-navigation hop if it uniquely hits an
enabled accessibility **link** and the next observed registrable domain
changes. Hops stay bounded to two; ambiguous or ungrounded clicks, skipped or
failed actions, and intervening non-link actions grant nothing. The native
gate uses `observe_tree_only()`, which normally omits screenshot bytes on
desktop and browser accessibility observations. Native coordinate grounding
now reuses the validated dimensions of the complete observation that triggered
the action; it does not rely on a JPEG inside the tree-only observation.
Regression perceptors deliberately omit those bytes, covering link grants and
target-text injection checks. Real browser/desktop navigation still needs
native-provider integration testing.

Validation (2026-10-16): domain unit fixtures cover malformed origins,
private suffixes, delayed links, unrelated actions and the two-hop bound;
engine tests cover all three codes, benign-screen fixtures, action counts and
human confirmation. `cargo test -p kai-core --test cua_native --test
safety_parity_e2e` passed **30/30 native tests** and **10/10 Phase 5 tests**,
including watch-mode waits, repeated positive monitor verdicts and
cancellation of a stalled monitor. Native scripted HTTP tests include empty
tree screenshots, a bounded two-hop chain, and denials for ambiguous/skipped
clicks and a third hop. No direct OpenAI key/GA API test or live
desktop/browser native-provider safety run,
or production screenshot-monitor evaluation has been reported. The separate
native monitor loopback suite passed 6/6 checks for opt-out, safe and unsafe
verdicts, errors, refusal without a person and cancellation. Production
recall, false-stop rate and cost remain unmeasured pending `kai bench` A/B
runs (two per side; target ≥95% fixture recall, ≤1% benign screens flagged).
Keep all switches off by default until these are measured.
