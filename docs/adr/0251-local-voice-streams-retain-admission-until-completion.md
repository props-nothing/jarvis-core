# ADR-0251: Local voice streams retain admission until completion

Status: accepted

Date: 2026-10-18

## Context

The owner selected warm-up progress, streaming playback and Fast/Balanced/Quality
controls after measured RX 9070 XT latency showed substantial cold-start cost and
whole-file buffering. Rust remains the control plane; VoxCPM2 remains replaceable.

## Decision

The existing owned pipe worker uses a versioned, bounded frame protocol for real
preparation stages, device metadata and 48 kHz mono signed 16-bit little-endian
PCM chunks. Warm-up runs both design and reference-conditioned streaming before
reporting ready. No percentage or completion time is invented. Preparation is an
explicit authenticated operation; opening the editor does not reserve GPU memory.

Fast/Balanced/Quality mean four/six/ten diffusion steps. Quality remains the
default for old libraries. The saved voice stores the speech setting; its generated
reference separately records the setting used to create that immutable identity.
Changing delivery quality does not invalidate an existing reference. Other engines
refuse this control.

The adapter retains its worker lock, admission and cancellation ownership through
stream EOF. Before first audio, GPU failure may retry once in the separate CPU
runtime. After first audio, failure ends the stream; it cannot replay from the
start through CPU or another provider. Cancellation/disconnect drops owned work.
Only successful EOF returns a worker to the warm cache. Bounded preview permits
and cancellation leases remain alive for the whole HTTP body.

Existing complete design/preview responses remain available. Opt-in design
streaming uses bounded NDJSON audio events and one complete event containing the
validated reference. A partial design cannot be saved. Speech/preview streaming
uses explicitly labelled PCM; full collectors wrap it in a valid WAV. Browser
Web Audio schedules chunks while generation continues, bounds queued data and
stops scheduled sources on cancellation or error. MP3 and existing WAV playback
remain supported.

Queue repair amendment, 2026-10-18: Console keeps one unfinished synthesis request
at a time; playback may overlap generation of subsequent items after the previous
HTTP stream ends. Local admission saturation is Busy, exposed as 503 with
Retry-After: 1. It does not open the provider breaker or switch to a backup. The
browser retries before audio with a bounded cancellable wait and keeps later text
in the same session even if an actual provider failure requires browser speech.
Automatic wake listening cannot interrupt preparation or a pending speech queue.
Explicit user interruption and the no-replay rule after partial audio remain.

## Verification

Required: malformed/truncated/oversized frame rejection, absent complete event,
quality validation and legacy defaults, admission held across streams, cancellation
before/after first audio, no retry after partial output, GPU/CPU preparation status,
browser playback before EOF, persistence/restart, and live AMD acceptance.
Native NVIDIA/macOS/Linux acceptance requires those hosts.

Windows RX 9070 XT release acceptance and main-profile activation passed on
2026-10-18, including actual browser Console playback and forced CPU fallback.
The [integration record](../research/integrations/voice-gpu-performance.md#implemented-warm-up-and-streaming-acceptance-2026-10-18)
retains measurements, evidence paths, failed harness attempts and remaining limits.

The same-day queue repair also passes real nine-segment AMD playback, transient
busy/rate-limit recovery and cancellation, plus fixture coverage for delayed
wake callbacks and browser-fallback late text. See the
[queue repair verification](../research/integrations/voice-gpu-performance.md#queue-repair-verification).
