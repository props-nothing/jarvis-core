# ADR-0250: Local voice GPU failure retries in an isolated CPU runtime

Amended by [ADR-0251](0251-local-voice-streams-retain-admission-until-completion.md):
streamed audio retains worker ownership and limits retry to before first audio.

Status: Accepted

Date: 2026-10-18

## Context

The owner requests AMD RX 9070 XT and NVIDIA voice design acceleration with CPU
fallback. [Runtime research](../research/integrations/voice-gpu.md) identifies
pinned Windows runtimes. Hardware presence alone does not prove usable inference.

## Decision

Extend the daemon-owned worker from [ADR-0249](0249-description-designed-voices-retain-the-generated-reference.md).
Keep the CPU environment and add an optional `gpu/venv` environment sharing the
same verified model. Explicit installation selects AMD or NVIDIA packages;
synthesis never installs software. Test GPU availability and tensor operations,
prefer the compatible adapter with most memory, then load the model there.

Rust retries once using the separate CPU interpreter after a GPU worker fails,
including startup, inference or its bounded attempt timeout. Kill the failed
worker tree before retrying. Retain the CPU choice until daemon restart. The
overall deadline remains three minutes, with at most ninety seconds for the GPU
attempt. Cancellation drops the owning future and never triggers fallback.
Both attempts use the same request and saved reference; no partial audio is sent.

Version the private pipe handshake as JVD2. Bound and validate backend/device and
GPU allocation metadata before accepting it. Report observed execution through
authenticated settings metadata, design responses and a small runtime-status
route. This exposes no paths, SDK errors, credentials or reference audio. UI says
detection is pending until actual runtime evidence arrives. Memory is PyTorch
allocation, not total board usage. Exact waveforms need not match across devices.

## Consequences

An incompatible/missing GPU or failed driver/kernel keeps voice generation usable
on CPU. GPU dependencies do not overwrite the known CPU environment. Offline
model checks, reference admission, process ownership and cancellation remain in
Rust. This is not an OS sandbox. Windows AMD receives local hardware acceptance;
NVIDIA fixtures do not substitute for NVIDIA hardware acceptance. Embedded Kokoro
remains the existing small CPU backup and cloud voices execute at their provider.
