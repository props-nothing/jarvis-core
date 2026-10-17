# ADR-0239: Prebuilt installation pins the release and digest

Status: Accepted

Date: 2026-10-17

## Context

P12-001 orders toolchain-free installation first. CI already publishes unsigned
archives for five platforms. The website bootstrap still builds from source.
Signed distribution, atomic updates and clean-machine acceptance are unfinished.

## Decision

Add separate opt-in `scripts/install-prebuilt.sh` and `scripts/install-prebuilt.ps1`
entry points. Require an exact release tag, package version and SHA-256 digest.
Use only the official repository's HTTPS release downloads, with bounded transfer,
native platform detection and integrity verification before archive reading or
execution. Materialize only the exact single regular executable into private
staging; archive paths are never extraction destinations. Check the program version
before delegating to the existing per-user installer and PATH setup.

Refuse an existing destination executable. This slice installs a first copy; it
does not implement schema-aware update or rollback. Never start a service or ask
for keys inside the bootstrap. Print the separate onboarding and doctor handoff.
Custom absolute destination plus no-PATH mode supports isolated verification.

The digest is supplied by the operator and does not replace publisher signatures.
Label this path unsigned and leave the website source bootstrap unchanged until
the signed default path and its native acceptance are ready. No insecure fallback,
automatic latest selection, arbitrary mirror or checksum bypass is provided.

## Consequences and acceptance

Modern OS tools suffice; Git, Rust, Node and Python are not installer dependencies.
curl >=8.4.0 is required to enforce size bounds even without Content-Length.
The [research record](../research/integrations/prebuilt-install.md) defines the
contract, falsification tests and native/live evidence required. Signed packages,
notarization, complete notices/SBOM, updates, clean-machine lifecycle and the
owner's voice-at-first-run decision remain open under P12-001/P12-002.
