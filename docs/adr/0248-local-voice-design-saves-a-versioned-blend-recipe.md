# ADR-0248: Local voice design saves a versioned blend recipe

Status: Accepted

Date: 2026-10-18

Extended by [ADR-0249](0249-description-designed-voices-retain-the-generated-reference.md):
the owner clarified that description-conditioned creation is the primary flow;
preset blending alone does not close P8-024.

## Context

P8-024 extends the P8-023 voice library with local preset blending. Each preview
and saved voice must use the same recipe, and editing weights must not allocate
an unbounded collection of native model sessions.

## Decision

Add an optional, versioned blend recipe to a saved voice. It contains a pinned
model revision, explicit pronunciation language and integer preset percentages
summing to 100. Domain validation bounds the document; the embedded adapter
validates supported revisions, presets and languages. Other adapters reject blends.
Existing schema-1 records without a recipe retain their behavior. Older builds
reject the new field rather than ignoring it, so downgrading requires removing
blended records with a compatible build first.

Compute style vectors from verified public presets for each admitted inference.
Use a fixed source order and defined float64-to-float32 arithmetic, with two shared
language engines and the existing single-inference permit. Preview, normal speech
and restart reconstruction all use this path. No derived voice files are persisted.

The existing atomic voice-library save owns recipe persistence and deletion.
An authenticated export route returns the saved recipe, name, model and controls
in a versioned document, excluding local provider identities and credentials.
Unsaved changes must be saved before export. Removing a referenced voice remains
blocked until default, channel and backup references are reassigned.

## Consequences

The owner can blend Heart and George, preview, save, export and delete the design
without cloud synthesis or training. Public preset/model cache files are shared
and are not removed with a recipe. Identical recipes reproduce the same style
tensor; native audio reproducibility is tested on the recorded host/runtime, not
promised bit-for-bit across different hardware. Text-description design remains
unavailable because no instruct-capable local engine has been selected.

This extends [ADR-0247](0247-voice-previews-and-saved-voices-use-the-same-validated-adapters.md)
and the bounded engine sharing in [ADR-0246](0246-embedded-speech-loads-only-pinned-local-assets.md).
