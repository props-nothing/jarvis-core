# ADR-0214: First run tests the model and required voice before saving

- Status: Accepted
- Date: 2026-10-14
- Scope: Owner-requested installation/setup review; selected ahead of the other open slices as the `P11-007` first-five-minutes work.

## Context

Source bootstrap still needs Rust/native build tools, while an installed native binary does not. Terminal setup previously worked only with local Ollama or manually supplied provider flags; keys echoed, voice was skipped by default and configuration could be saved before the provider answered. The owner explicitly chose an ElevenLabs key as a prerequisite for finishing new setup, and supplied `liN4AsrhF3CylqMDWK6p` as the desired voice ID.

## Decision

- First launch and `jarvis init` reuse the console's existing provider presets, model listing and bounded completion test. No stale model catalogue or new wire adapter.
- Hidden terminal key entry uses pinned Apache-2.0 `rpassword` 7.5.4. Terminal failure refuses; noninteractive callers supply absolute key files or provider environment variables. Keys are never command-line values, logged or written in configuration.
- New CLI setup requires a usable ElevenLabs key with text-to-speech access and a nonempty first audio chunk from one short test. It uses voice ID `liN4AsrhF3CylqMDWK6p` unless explicitly overridden. The voice identifier is public configuration, not authentication. Phone-agent permissions are not needed for ordinary setup.
- The model test and voice test send fixed synthetic text and may consume credits. They make no tool calls, perform no retry, and require no extra approval step. Failed tests, bad folders and invalid configuration do not persist new keys/configuration or replace a prior configuration.
- Offer existing home `Development`, `Projects` and `code` folders as suggestions. No suggestion is a grant: only a path/number explicitly entered or `--workspace` is saved. Empty selection is allowed; duplicate canonical roots are collapsed. No whole-home default, folder creation, coding-agent install or automatic standing trust.
- Existing profiles, manually configured headless operation, later voice-key removal and browser fallback remain usable. Requiring a cloud key is **new CLI onboarding policy**, not a daemon/domain prerequisite; `FR-INSTALL-002` still applies to the core runtime. No configuration schema or approval/trust model change.
- Source installer preflights distinguish build dependencies from optional Node/browser, Docker/sandbox and Tesseract/OCR tools. Apache-2.0 is selected for original code; public signed binaries and clean native installation proof remain independent gates.

## Consequences

Setup fails early with a concrete provider message instead of saving a broken first conversation. A user without ElevenLabs access cannot complete new guided setup, per the owner's confirmed preference; the manual/headless path remains. The selected voice might not be available to every account, so setup tests access and allows an override rather than claiming universal availability. Existing explicit voice IDs are unchanged; profiles using an implicit provider default pick up the new ID.

## Alternatives

Optional voice was recommended but rejected by the owner for new setup. Making the entire daemon depend on ElevenLabs was rejected: it would break existing/headless operation and provider replaceability. Automatically granting discovered code folders was rejected: convenience is not permission. Auto-installing Rust/native tools without opt-in or claiming unsigned/source bootstrap as a native binary installer was rejected.

## Verification And Revisit Conditions

`cargo test -p jarvis-cli init::`: 20 tests passed on Windows, including ten new helper/contract tests for provider choices, key redaction, missing tests/keys, exact default voice, explicit overrides, no folder widening, model-list failure, refused auth/quota, empty audio and preserving configuration on failure. Synthetic loopback services only, no credentials or live account calls. Workspace formatting and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` passed in the coordinated Actions/release validation. Its latest `cargo test --workspace --all-features --locked --no-fail-fast` run recorded 2,913 passed, 3 failed, 1 ignored: only the existing phone-disclosure assertions reproduce; the unrelated coding-task timing test also passed the full rerun. CLI/init/storage passed; the full release gate remains red. Native hidden-prompt checks, clean Windows/macOS/Ubuntu installs and synthesis with the selected voice on an owner account remain open. Revisit mandatory cloud onboarding if the owner asks for a no-account guided path or a verified local voice adapter is delivered.

Related research: [terminal keys](../research/integrations/rpassword.md), [ElevenLabs](../research/integrations/elevenlabs.md), [model providers](../research/integrations/model-providers.md), [source prerequisites](../research/integrations/source-install-prerequisites.md).
