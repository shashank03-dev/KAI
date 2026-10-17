# Mac GUI testing handoff

**Status: prepared, not executed.** No test session, live provider inference or account sign-in was performed during implementation.

## Start entirely in the GUI

1. Open `dist/Kai.app` (or the installed Kai application). If macOS requires it, use Finder's Control-click → Open.
2. Enable Kai in System Settings → Privacy & Security → Accessibility and Screen Recording. Restart Kai if macOS requests it. A changed ad-hoc signature may require toggling permissions again.
3. Open Settings → Accounts & models. Press **Connect ChatGPT / Codex** and complete the provider's browser sign-in. The Mac package includes the native Codex helper when it was installed on the build machine; Kai does not copy credentials.
4. Refresh account and models. Choose the exact **gpt-5.6-luna** entry if your provider offers it. If absent, do not assume GPT-6 Luna is equivalent: model access must be settled before that requested evaluation. The exact-ID field also supports provider IDs outside a listed catalog, but the subscription testing preset requires a listed selection.
5. Press **Prepare Mac GUI testing with selected model**. This prepares the safe Calculator task without starting it. Review the selected model and task, then press Run when testing is authorized.
6. Watch **Steps**, **Kai's view** and **Earlier frames**. In a narrow window, use the Screen tab. Inspect the requested action, screenshot and timing rather than relying on the model's final statement. The first task's independently visible expected result is **391**.
7. Use Pause to inspect, send a steering message to redirect the next step, or Stop to end the run. The global stop shortcut and top-left screen corner remain available. Never bypass a sign-in, payment or password request just to continue a test.
8. Review History and retain the local trace/screenshots needed to explain success or failure. Recording is enabled by the preset; disable it later in Privacy if desired.

The preset uses native desktop control, blocks shell actions and recognized terminal/IDE input, disables team/executor/procedure/plan-cache shortcuts and uses one selected model. No terminal task is needed to run this session. It is not an OS security sandbox for arbitrary embedded command interfaces.

## Other providers

- **Claude Code:** press Connect Claude Code, complete its browser flow, refresh, and choose a Claude alias or exact model ID. Its helper must be installed; the setup button opens official installation guidance. Restricted decision mode requires Claude Code 2.1.259 or newer. Aliases are documented defaults rather than discovered account entitlements.
- **OpenAI / AI Gateway:** choose the provider; enter its API key in Accounts & models and refresh its catalog there. These use API billing, not a ChatGPT subscription.
- **Compatible server (DeepSeek, OpenRouter or another OpenAI-compatible endpoint):** enter its HTTPS `/v1` base URL and API key, select Compatible server and refresh. Local HTTP servers on loopback may omit a key. A changed cloud origin requires its own key to be entered again. Choose a model that accepts screenshot images; text-only discovery entries cannot perform this task.
- **Local models:** use the existing local model catalog/download/start controls. A running compatible local server can also be discovered through the compatible endpoint field.

## Deferred acceptance checklist — do not mark complete from compilation

- [ ] Browser sign-in success, cancel, timeout, reconnect and logout.
- [ ] Catalog refresh, pagination, exact IDs and model selection after provider changes.
- [ ] API credential rejection and no reuse across endpoint origins.
- [ ] Actual GPT-5.6 Luna screenshot decision and Calculator task.
- [ ] Mouse/keyboard freshness, Retina coordinate accuracy and recovery crops.
- [ ] Pause/resume, stop/kill shortcut and steering during a slow model request.
- [ ] GUI-only shell refusal and terminal/IDE foreground refusal.
- [ ] Independent task outcome check versus model-reported completion.
- [ ] Local trace/history review, timing, model identity and privacy controls.
- [ ] Only afterward: repeatable task matrix and any GPT-6 Luna comparison.

Record OS/build/helper/model versions and classify model failures separately from permissions, account limits, harness faults and deliberate safety stops. Use repeated identical tasks to measure improvement; a successful first task does not establish a general reliability score.
