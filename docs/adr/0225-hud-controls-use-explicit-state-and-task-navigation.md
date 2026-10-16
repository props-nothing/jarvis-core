# ADR-0225: HUD controls use explicit state and task navigation

- Status: Accepted
- Date: 2026-10-16
- Owner-selected follow-up to the [HUD review](../quality/hud-ui-ux-review-2026-10-16.md).

## Decision

Prioritize the owner's requested HUD repair before the next provider acceptance slice. Settings retain unsaved, non-secret drafts in memory across saves and navigation. Secrets remain write-only and never enter browser persistence. The daemon reports configuration changes since startup; the browser is not the authority for restart state. Model address, model name and a replacement credential are committed together using an immutable private credential file and an atomic configuration replacement. Failed validation cannot replace the active credential.

Organize outbound capabilities under **Apps & tools**, messaging under **Channels**, and inbound access under **External access**. MCP is an advanced connection method inside Apps & tools, not a competing product category. Trust controls describe their effect on approvals and never claim to sandbox a server. Search includes setting names and integration synonyms. Operations separates Activity, Automations and Knowledge. Following the owner's same-day design correction, the face has the primary desktop space with a compact conversation panel. Mobile keeps a compact presence where height permits and an accessible composer. See the [design refinement](../quality/hud-design-direction-2026-10-16.md).

Adopt TypeScript, Vite and React incrementally for a contained integration catalog. Legacy settings, voice, approval and canvas adapters remain in place during migration. The catalog receives plain data and navigation callbacks; it cannot execute tools or decide policy. Build artifacts are checked in and embedded by Rust, so installed users do not need Node or an internet connection. CSP remains same-origin with no inline scripts, styles or remote assets. Do not add a router, global state framework, SSR or a second component system without a concrete need. Accessible native controls suffice for this slice; Radix is a researched option for future complex widgets, not an unused dependency.

## Limits

Startup comparisons describe saved configuration, not provider health or proof of a live connection. Existing live policy operations retain their applied response. This work does not migrate durable conversation storage or replace existing provider authentication contracts. Browser history must disclose retention and storage failures. Real provider consent and platform-native behavior require their own acceptance evidence.

## Verification

Record focused behavioral tests, the production frontend build, browser checks and the Rust baseline in TODO.md. Keep the memory inspector suite in CI. Test literal untrusted text and the exact static-asset allowlist without weakening checks on handwritten scripts because a framework contains general DOM implementation code.
