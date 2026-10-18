# ADR-0247: Voice previews and saved voices use the same validated adapters

Status: Accepted

Date: 2026-10-18

Extended by [ADR-0248](0248-local-voice-design-saves-a-versioned-blend-recipe.md)
for reproducible local blends and saved-recipe export.

## Context

P8-023 needs a profile-local voice library, preview and controls that affect real
speech. Speech destinations and private keys are already configured separately.
Preview must not sound successful by silently selecting a backup voice.

## Decision

Keep provider-neutral voice records and structural validation in jarvis-core,
profile persistence in jarvis-storage and capability validation in jarvis-voice.
The daemon owns an immutable active routing snapshot and a revisioned library.
Authenticated library saves validate all records, atomically persist the new
revision, then publish the routing snapshot. Existing requests retain their
snapshot. Missing/corrupt persisted state is never silently replaced.

The library lives in private profile configuration as voice-library.json. On its
first use, existing configured speech providers seed named voices and preserve
the existing default/backup order. A save does not change provider credentials,
endpoints or account settings. Stale revisions return conflict. Provider changes
continue to require a daemon restart; ordinary voice edits apply to new speech.

Preview uses the selected draft with no fallback and never saves it. Model-level
capabilities control both the editor and server validation. Unsupported controls
and unavailable channel assignments are rejected, not ignored. Console has an
optional voice override; other channels become selectable only when their real
delivery adapters support these records. Telegram text and ElevenLabs agent calls
retain their existing behavior with explicit explanations in the editor.

## Consequences

Voice speed/style changes can be heard before saving, and playback reflects what
will be saved. Embedded adapters share bounded engine resources across edits;
preview cannot create unbounded native sessions. Cloud text egress remains an
explicit provider choice. This adds no cloning, voice design, telephony routing
or Telegram audio implementation. Those channel portions of P8-023 remain open.
