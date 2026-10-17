# Real coding-agent website acceptance

This opt-in gate covers the remaining complete-website journey in `P7-004` and
`P7-008`. It consumes the selected account's model usage. Ordinary CI does not run
it. Use an already signed-in Codex account; fresh sign-in and desktop notification
delivery need separate evidence.

## Run

Build the current daemon, then pass the absolute entry point of the already
installed, pinned Playwright verifier:

```text
cargo build -p jarvisd --locked
node scripts/accept-coding-website.mjs --run-live --playwright /absolute/path/to/playwright/index.mjs
```

The verifier requires Playwright `1.64.0-alpha-1790635538000` and its matching
installed Chromium headless shell. A browser launch preflight runs before any
account-backed work. If it reports a missing executable, install that pinned
package's browser with `node /absolute/path/to/playwright/cli.js install chromium --only-shell`
and rerun. The harness itself does not install a browser, upgrade dependencies, or alter an owner
profile. The daemon uses the existing pinned `codex-acp 2.2.2` command and agent
authentication. See [the contract refresh](../research/integrations/coding-delivery.md).

If the agent times out or fails after creating useful work, retain the failed
report and explicitly continue that disposable task:

```text
node scripts/accept-coding-website.mjs --run-live --playwright /absolute/path/to/playwright/index.mjs --continue-from artifacts/reviews/coding-website/run-EXAMPLE
```

Only direct `run-*` directories under this harness's evidence folder are
accepted after resolving symlinks. A continuation requires a failed original
report and a failed, cancelled or interrupted job still belonging to its original
work folder. It goes through a fresh approval and must receive a new task ID.
Supplied files are checked, never reset. The original task/report remain intact;
new diagnostics and screenshots go in a `continuation-*` subdirectory. A successful
continuation is separate evidence and never converts the initial timeout into success.

## What must pass

The harness creates a unique disposable profile and permitted folder beneath
`artifacts/reviews/coding-website/`. Through the real authenticated HTTP API, it
denies a coding request, proves a resume is refused and no job exists, then approves a new request.
The real agent builds a responsive task board. JARVIS independently runs supplied
behavior checks; their source and an unrelated owner file must remain unchanged.

An approved managed process serves only the generated site's four files on
loopback. Separate isolated Chromium contexts exercise desktop and mobile
emulation: add a task, toggle completion, filter tasks, reject whitespace,
display HTML-looking input literally, reload persisted state, and detect
horizontal overflow and browser errors. Screenshots are retained for visual
inspection. Mobile emulation does not establish physical-device acceptance.

Finally the harness explicitly stops the managed preview, checks terminal
process state and a closed port, rechecks unrelated files, and shuts down the
disposable daemon. Failure at any stage leaves a failed report and retained work
for diagnosis. A running agent is allowed up to the test's explicit ten-minute
deadline; this does not change JARVIS's no-default-deadline behavior.

## Evidence and boundaries

Each run retains `verification.json`, desktop/mobile PNGs when reached, the
generated website, and local daemon/profile state. Keep this directory ignored;
do not commit its credential, database, or provider output. The report names the
platform, adapter/browser versions, daemon binary hash, task/process identities,
verified checks and cleanup result. Keep failed attempts alongside passing ones.

This proves the tested real-agent website journey on the recorded host. It does
not establish automatic natural-language orchestration of all steps, other agents,
native macOS/Linux behavior, fresh browser sign-in,
notification reception, an OS sandbox, or structured parent/child run semantics.
See [the coding guide](../user/code-agents.md) and [active acceptance](../../TODO.md).
