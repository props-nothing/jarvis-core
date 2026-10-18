---
integration: voice-design
status: researched
last_verified: 2026-10-18
owners: []
selected_spec_version: Kokoro ONNX v1.0 revision 1939ad2a8e416c0acfeecc08a694d14ef25f2231
selected_sdk: none; existing ort 2.0.0-rc.12 and misaki-rs 0.6.0
---

# Local voice blending (P8-024)

## Question and official evidence

Can the embedded engine reproduce a weighted preset blend from a portable recipe,
without another model, provider, executable or cloud request?

Official sources fetched on 2026-10-18 UTC:

- https://raw.githubusercontent.com/hexgrad/kokoro/main/llms.txt and
  https://raw.githubusercontent.com/hexgrad/kokoro/main/llms-full.txt: both 404.
- https://raw.githubusercontent.com/hexgrad/kokoro/main/README.md
- https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/raw/1939ad2a8e416c0acfeecc08a694d14ef25f2231/README.md
- https://raw.githubusercontent.com/hexgrad/kokoro/6d87f4ae7abc2d14dbc4b3ef2e5f19852e861ac2/kokoro/pipeline.py
- https://api.github.com/repos/hexgrad/kokoro/commits?path=kokoro/pipeline.py&per_page=1
- https://github.com/hexgrad/kokoro/releases (no published releases; presumed v0.9.4 tag is absent)
- https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/commits/main

The official pipeline averages multiple preset tensors and accepts a caller-supplied
tensor. Pronunciation is chosen by the language pipeline separately. The pinned
ONNX contract takes a float32 style row of 256 elements selected by token count.
JARVIS retains its existing ONNX row indexing and Rust phonemizer; it does not copy
the Python implementation or install its eSpeak dependency. Existing asset licenses,
hashes, native-runtime trust and versions remain in [embedded speech](embedded-speech.md).
No relevant deprecation or required dependency change was observed.

## JARVIS decisions and limits

Use a version-1 linear weighted blend of the already pinned Heart and George
presets. Integer percentages sum to 100; zero-weight endpoints are permitted.
Compute each element in a fixed preset order in float64, then round to float32.
Equal weights implement the documented average; arbitrary convex weights are a
JARVIS extension to that tensor contract, verified against an independent calculation.
The recipe records the model revision and explicit en-us/en-gb pronunciation.
Reject unknown revisions, presets, languages, fields, duplicate sources and invalid
totals before native work. Do not silently substitute a preset or send a blend to
a cloud adapter. Kokoro does not support text-description design. A separate
[VoxCPM2 adapter](voice-description-design.md) provides that capability. Subjective resemblance and accent quality are not guaranteed.

Keep recipes in the existing private voice library with name and speed. Export a
versioned JSON recipe without provider identities, credentials, paths or audio.
Deletion removes the recipe; shared public model/preset assets remain reusable.
Retain two bounded language engines and one inference permit across every recipe;
never cache a native engine or derived tensor per blend. Preview stays exact and
unsaved. Existing authentication, 300-character preview limit, timeouts, audio cap,
revision conflicts and fallback policy apply. There are no new auth scopes, external
API endpoints, webhooks, pagination, retries, account mutation or synthesis charges.
Asset downloads retain the existing explicit provider policy and hash verification.

## Discriminating verification

Compare actual style rows with independent weighted sums, including pure-preset
endpoints, reordered sources and different token counts. Invalid recipes and cloud
assignment must fail before work; mutation of the capability guard must break its
test. Use real cached native synthesis and browser playback to compare distinct
weights, save a blend, use it for console speech, restart and reproduce audio,
export the exact saved recipe, then delete it and prove it is absent after restart.
Check desktop/mobile layout, keyboard input and cancellation of stale previews.
Windows live evidence does not imply native macOS/Linux acceptance.


## Live Windows evidence (2026-10-18)

`node scripts/accept-voice-design.mjs --run-live --cache <prepared-cache> --playwright <absolute-index.mjs>`
passed with actual native synthesis and Edge 155.0.4283.62. Evidence is retained
under `target/voice-design-acceptance/profile-uuf3Ir`: report, WAVs, desktop/mobile
screenshots and export. A 35/65 blend produced 3.85 seconds of audio and played to
completion. Pure endpoints match preset output exactly; changing weights changes
audio; reordered sources do not. Save/Console playback and restart reproduce the
same bytes. Export matches the recipe and deletion survives restart. Removing the
local-only recipe guard made its falsification test fail; the guard was restored.
