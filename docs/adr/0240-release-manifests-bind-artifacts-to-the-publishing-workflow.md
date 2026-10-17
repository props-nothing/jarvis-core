# ADR-0240: Release manifests bind artifacts to the publishing workflow

Status: Accepted

Date: 2026-10-17

## Context

P12-001 requires signed installation and safe lifecycle operations. ADR-0239's
explicit checksum path establishes byte integrity but has no publisher signature.
Updates must not accept an authentic old artifact relabelled as a newer release.

## Decision

The release job signs a strict versioned manifest with Cosign and GitHub Actions
OIDC. It binds the official repository, tested commit, exact tag, version and the
complete five-platform archive set. Only this repository's release workflow on
main or the selected version tag is an accepted signing identity. Signing and
verification fail closed before publication; no persistent signing key is needed.
Publication verifies the uploaded manifest/bundle bytes alongside all archives.

Bootstrap signed mode uses a checksum-pinned standalone Cosign executable and
requires the expected issuer/identity, valid bundle, exact requested manifest
tag/version and selected archive hash before reading or running JARVIS. A failed
signature never triggers an unsigned fallback. ADR-0239's explicitly supplied
digest path remains available for historical unsigned releases.

Sigstore workflow authentication is distinct from native Authenticode signing or
Apple notarization. No OS trust prompt is removed or signing identity invented.
No verifier SDK crosses Rust domain boundaries; cryptography remains in the pinned
external tool. Rust continues to own profile and deployment lifecycle decisions.

## Consequences

Release verification needs no end-user account or development toolchain, but adds
a temporary verifier download and public trust-metadata access. Repository/bootstrap
and verifier pins remain trust roots, changed only through reviewed code. A
compromise requires pausing publication, fixing the workflow or verifier pin,
revoking affected releases and explicitly distributing a corrected bootstrap;
an existing signature is not erased by deleting its public asset. Neither an
old signature nor a version label authorizes automatic downgrade or data loss.

The [research record](../research/integrations/release-signing.md) specifies the
current official contracts and falsification/live gates. Native OS certificates,
atomic update/schema recovery and clean-machine onboarding remain separately
tracked requirements until their evidence passes.
