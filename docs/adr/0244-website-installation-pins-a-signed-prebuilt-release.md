# ADR-0244: Website installation pins a signed prebuilt release

Status: Accepted

Date: 2026-10-18

## Context

The owner selected the remaining P12-001 website default. ADRs 0239 and 0240
already provide signed prebuilt installation, while website scripts still compile
source. Release a176e79843a22eae2802d97aafba74a0bf6e282f has recorded hosted native
publication, installation and lifecycle evidence.

## Decision

Replace the website's default Bash/PowerShell source builders with standalone
signed-only installers pinned to that release and version 0.1.0. Preserve the
existing verifier pins, workflow identity, strict manifest checks, bounded
transport/extraction and first-install exclusion. There is no unsigned fallback,
mutable latest selection, automatic service start or bootstrap key collection.
Keep the explicit From source tab. Legacy source-builder environment overrides
are rejected with migration guidance instead of silently selecting different work.

This supersedes ADR-0239's instruction to leave the website default unchanged.
Changing the pin is a reviewed website release with native tests. The website
remains the bootstrap trust root; payload signatures establish publisher identity,
not native OS certificate signing. Rust still owns installation and maintenance.

## Consequences

Default installation needs OS utilities and curl >=8.4, without Git, Rust, Node or
Python. Existing installations are refused before download and use the separate
update flow. Setup remains explicit and retains the owner's ElevenLabs decision.
Website implementation, deployment, native acceptance and clean-machine acceptance
are recorded separately. See the [research and tests](../research/integrations/website-signed-install.md).
