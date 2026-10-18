# ADR-0249: Description-designed voices retain the generated reference

Status: Accepted

Amended by [ADR-0250](0250-local-voice-gpu-failure-retries-in-an-isolated-cpu-runtime.md)
for optional Windows GPU execution and isolated CPU retry.

Date: 2026-10-18

## Context

The owner clarified that P8-024 must create a voice from a written description,
then save and select that identity. Kokoro blending alone is insufficient.
[Research](../research/integrations/voice-description-design.md) identifies
VoxCPM2's native description conditioning and reference reuse; a Windows CPU
probe generated actual audio successfully with the pinned 2.0.3 SDK.

## Decision

Keep the Rust control plane and embedded Kokoro backup. Add an explicitly
configured `local-voxcpm2` adapter with an absolute private runtime directory.
JARVIS starts a hidden, pipe-only Python worker on demand, owns its process tree,
and kills it on cancellation, timeout or protocol failure. There is no HTTP speech
server or independent startup task. This worker is a trusted local dependency,
not an OS sandbox. The model is shared across successful requests, with one active
request and two bounded waiting requests. Preparation verifies pinned model files;
generation forbids downloads, disables telemetry and discards raw SDK diagnostics.
Use a separate, version-pinned inference environment; do not copy VoiceStudio code.

Generate a short synthetic reference from a description, sample text and seed.
Only a reference issued by this daemon, or already in its saved library, may be
introduced through the save API. Unsupported engines reject designed references.
The selected reference, description, seed and model revision persist together in
the atomic voice-library JSON. Bound a reference to ten seconds of mono PCM;
raise the file cap to the existing authenticated API's 8 MiB limit. This keeps
save, export and deletion atomic without introducing a second object lifecycle.
The tradeoff is a larger settings response and fewer maximum-duration designs
per library. Duplicated entries can reuse the same generated design.

Later speech conditions on the saved reference, rather than creating a new voice
from the description on each sentence. Export includes the synthetic reference;
deletion removes it with the record. Unsaved references live only in bounded
daemon/browser memory. No upload, human-voice cloning or fine-tuning is added.
Existing libraries remain readable; older builds reject the new fields.

Local description synthesis needs a longer, bounded deadline than hosted speech.
The adapter declares its deadline, capped by the router, while ordinary providers
retain 25 seconds. Preview/generation can be cancelled from Settings. The UI leads
with description, generation, playback and saving; preset mixing is labelled
separately. Saved controls unsupported by this engine are refused.

## Consequences

The feature requires about 5 GB of public model assets plus a managed inference
environment. CPU generation is slower than Kokoro; quality is previewed rather
than promised. Windows is the measured host; other native platforms and GPU
acceleration require their own acceptance. Model/runtime setup is explicit and
separate from offline synthesis. This extends
[ADR-0248](0248-local-voice-design-saves-a-versioned-blend-recipe.md) and
[ADR-0247](0247-voice-previews-and-saved-voices-use-the-same-validated-adapters.md).


### Installation placement

Public Python runtime and model files are installed outside managed profile
folders (`%LOCALAPPDATA%/JARVIS-VoiceDesigner` on Windows). Profile permission
maintenance must not repeatedly traverse this large dependency tree. The private
library retains generated samples; the public inference installation does not.

### Setup feedback

The authenticated voice metadata includes an optional `design_unavailable_reason`
from the adapter. This cheap local file/size check starts no worker and exposes
no runtime path or credential. It gates Generate in the editor and gives repair
instructions before a request; inference still verifies all asset hashes. A
successful preflight is not a guarantee that inference will succeed.
