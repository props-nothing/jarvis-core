# JARVIS Documentation

**From your first conversation to the boundaries that keep you in control.**

[Home](../README.md) / [Quick start](user/quick-start.md) / [Settings](user/settings.md) / [Roadmap](../ROADMAP.md)

## Choose Your Path

| You want to... | Open... |
| --- | --- |
| Start JARVIS and get a first answer | [Quick start](user/quick-start.md) |
| Run on Windows, macOS or a Linux server | [Platforms and verification](user/platforms.md) |
| Pick a model, voice, folders and permissions | [Settings](user/settings.md) |
| Connect accounts with browser sign-in | [Connections and persistent GitHub OAuth](user/connections.md) |
| Connect Gmail and Calendar | [Google](user/google.md) |
| Clean up files, import contacts and update workbooks | [Files and recovery](user/files.md) / [contacts](user/contacts.md) / [spreadsheets](user/spreadsheets.md) |
| Reach JARVIS from your phone | [Telegram](user/telegram.md) / [owner phone calls](user/phone.md) |
| Add capabilities without rebuilding the core | [Abilities](user/abilities.md) / [MCP](user/mcp.md) |
| Connect an editor, agent or script | [Connect apps](user/connect-apps.md) |
| Give recurring work a goal and context | [Projects](user/projects.md) / [triggers](user/triggers.md) |
| Find a relevant remembered fact and its source | [Memory and current recall limits](user/memory.md) |
| Teach an approved procedure | [Skills](user/skills.md) |
| Delegate a coding task | [Coding agents and current limits](user/code-agents.md) |
| Track or stop delegated native work | [Native sub-agents](user/sub-agents.md) |
| Measure whether a change helped | [Evaluations](user/evals.md) |
| Build and contribute | [Development setup](development/getting-started.md) / [contributing](../CONTRIBUTING.md) |

> **Reading a specification?** Architecture and product requirements describe the target contract, not a feature checklist. The [README](../README.md#built-experimental-next), [roadmap](../ROADMAP.md) and [acceptance backlog](../TODO.md) distinguish shipped paths, partial implementations and unverified work. Research and ADRs retain their dated evidence.

## Understand The Core

1. [Product requirements](product/requirements.md)
2. [Architecture overview](architecture/overview.md)
3. [Repository layout](architecture/repository-layout.md)
4. [Roadmap](../ROADMAP.md)
5. [Implementation backlog](../TODO.md)
6. [Glossary](glossary.md)

## Architecture

**One owner of durable state. Replaceable adapters at the edges.**

- [Runtime and models](architecture/runtime-and-models.md)
- [Tools, MCP, and connectors](architecture/tools-and-connectors.md)
- [Memory and context](architecture/memory-and-context.md)
- [Identity and workspaces](architecture/identity-and-workspaces.md)
- [Storage](architecture/storage.md)
- [Events and workflows](architecture/events-and-workflows.md)
- [Voice and telephony](architecture/voice-and-telephony.md)
- [Security and threat model](architecture/security.md)
- [Protocols](architecture/protocols.md)
- [Domain/API contracts](api/contracts.md)
- [Conceptual data schema](data/schema.md)

## Decisions

- [ADR index](adr/README.md)

## Engineering

- [Getting started](development/getting-started.md)
- [External integration research](development/external-research.md)
- [Evaluated prototypes](research/evaluated-prototypes.md) — what was built, measured, rejected, and where the findings live
- [Integration research records](research/integrations/README.md)
- [Testing strategy](development/testing.md)
- [Definition of done](development/definition-of-done.md)
- [Acceptance tests](quality/acceptance-tests.md)
- [Historical evidence and original ledger slices](history/README.md) — dated findings, not the active task queue

## Operations

- [Installation and release](operations/install-and-release.md)
- [Observability and diagnostics](operations/observability.md)

## Research And Migration

- [Upstream architecture patterns](research/upstream-patterns.md)
- [Competitive gap review: Hermes Agent and OpenClaw (2026-10-17)](research/competitive-gap-2026-10-17.md)
- [Voice backup and local voices (2026-10-17)](research/voice-backup-and-local-voices-2026-10-17.md)
- [Integration research index](research/integrations/README.md)
- [Python prototype migration](migration/python-prototype.md)
- [Third-party provenance](../THIRD_PARTY.md)

## Authority

When documents conflict:

1. An accepted or superseding ADR controls the decision it covers.
2. Product requirements control externally observable goals.
3. Subsystem architecture controls ownership and invariants.
4. `TODO.md` controls implementation order, not architecture.
5. Research records control dated facts about external systems but must be refreshed before implementation.

Update links and affected documents in the same change. Run `node scripts/check-docs.mjs` from the repository root and `node --test scripts/test-docs.mjs` for the structural guard; it checks local targets, heading anchors and canonical ADR identifiers, not external URLs or semantic freshness. Do not create a second "master spec" that drifts from this set.
