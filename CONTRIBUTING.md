# Contributing To JARVIS

**Build capabilities. Preserve authority. Leave evidence.**

[Home](README.md) / [Development setup](docs/development/getting-started.md) / [Architecture](ARCHITECTURE.md) / [Backlog](TODO.md)

JARVIS is a working assistant under active development. Contributions should improve a runnable slice of the Rust platform, not turn the Python reference prototype into a second product. The [backlog](TODO.md) sets the order; the [engineering rules](AGENTS.md) set the boundaries.

## Pick A Contribution

| Change | What good looks like |
| --- | --- |
| Fix a bug | A small reproduction, a focused fix and a test that fails without it. |
| Add a capability | A named acceptance slice, policy-controlled effects and a real reachable client path. |
| Integrate a provider | Current official evidence, a thin adapter, failure-path tests and honest live-verification status. |
| Improve documentation | Exact commands, working links, clear prerequisites and no claims stronger than the evidence. |
| Change a boundary | An ADR explaining ownership, compatibility and the tradeoff before implementation. |

Original JARVIS code and documentation are licensed under [Apache-2.0](LICENSE). Contributions intended for inclusion are submitted under the same terms unless explicitly stated otherwise; only submit material you have the right to license. Preserve [NOTICE](NOTICE) and [third-party provenance](THIRD_PARTY.md). The former prototype's CC BY-NC 4.0 terms are not replaced by this license.

## Before You Change Code

1. Read [AGENTS.md](AGENTS.md), [product requirements](docs/product/requirements.md), and the relevant architecture document.
2. Select the first eligible unchecked task in [TODO.md](TODO.md).
3. If the task touches an external system, complete [the external research workflow](docs/development/external-research.md) and add or refresh its dated record.
4. If the task changes a durable boundary, add or supersede an ADR before implementation.
5. State one falsifiable hypothesis and choose the cheapest check that can disprove it.

## Change Shape

- Deliver a small end-to-end behavior, not disconnected interfaces and placeholders.
- Keep orchestration out of HTTP handlers, CLI commands, and UI components.
- Keep provider SDK types inside adapters.
- Put domain vocabulary and ports in `jarvis-core`; put use-case coordination in `jarvis-application`.
- Keep platform-specific service, filesystem, keychain, and audio details behind explicit ports.
- Prefer process isolation for third-party runtimes and untrusted extensions.
- Do not refactor the prototype while building the Rust platform unless a task explicitly names it.

## Evidence

A change is complete only when it has the evidence required by [definition-of-done.md](docs/development/definition-of-done.md). In general:

1. Run the narrowest behavior test.
2. Run formatting and linting for touched languages.
3. Run the relevant package/workspace tests.
4. Run contract or live tests when behavior belongs to an external provider.
5. Update docs, the integration research record, and `TODO.md` in the same change.

Never report a mock-only integration as operational. Label tests clearly as unit, contract, integration, live, platform, or end-to-end.

## Rust Baseline

Every normal Rust change must pass:

```text
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Security-sensitive state transitions, approvals, authorization, path handling, webhooks, secret redaction, and idempotency require negative and falsification tests.

## API And Data Changes

- Version externally consumed protocols.
- Generate OpenAPI and client types from one source where practical.
- Use additive database migrations and test upgrade from the previous released schema.
- Separate a schema migration from destructive cleanup by at least one compatible release.
- Back up local state before a migration that cannot be trivially reversed.
- Add idempotency semantics before retrying an effectful operation.

## Integration Changes

- Keep each connector thin and manifest-driven.
- Test credentials and connectivity before persisting a configured account.
- Implement pagination, rate limits, refresh/re-auth, revocation, and redacted diagnostics as part of the connector, not as later polish.
- Capture sanitized real wire fixtures when terms permit. Synthetic fixtures alone prove only self-consistency.
- Never put access tokens, refresh tokens, API keys, cookies, phone numbers, transcripts, or personal memories in fixtures.

## Documentation

- Link to canonical documents instead of copying sections into multiple files.
- Record assumptions and unresolved questions explicitly.
- Date claims about fast-changing external services.
- Use Mermaid for architecture flows and state machines.
- Update an ADR when a decision changes; do not rewrite accepted history without marking it superseded.

## Pull Request Checklist

- [ ] Scope maps to one or more `TODO.md` task IDs.
- [ ] Relevant official docs and `llms.txt` indexes were checked and recorded.
- [ ] No credentials, personal data, generated state, recordings, or transcripts are included.
- [ ] Boundary and failure-path tests exist.
- [ ] Formatting, linting, tests, and platform checks pass.
- [ ] Public API, migrations, security behavior, and operator docs are updated.
- [ ] Task checkboxes reflect evidence, not intent.