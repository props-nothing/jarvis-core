# Competitive gap review: Hermes Agent and OpenClaw (2026-10-17)

[Home](../../README.md) / [Documentation](../README.md) / [Upstream patterns](upstream-patterns.md) / [Roadmap Phase 12](../../ROADMAP.md#phase-12-competitive-catch-up) / [Backlog P12](../../TODO.md#competitive-catch-up-p12)

The owner asked how JARVIS compares with Hermes Agent and OpenClaw and asked that the gaps become roadmap and backlog work. This record keeps the comparison and its sources. The sources are product pages and a third-party comparison: under `docs/development/external-research.md` they are **leads, not authoritative sources**. Each P12 item that touches an external API, protocol or provider still needs its own research record from official documentation before implementation.

Accessed 2026-10-17. Version and star figures are as reported on that date and go stale quickly.

## Sources

| Source | URL | Kind |
| --- | --- | --- |
| Hermes Agent product site | https://hermes-agent.org/ | vendor marketing |
| What is OpenClaw? (DigitalOcean) | https://www.digitalocean.com/resources/article/what-is-openclaw | third-party article |
| OpenClaw vs Hermes Agent (Composio, 2026-09-15; OpenClaw v2026.9.4, Hermes v0.21.2) | https://composio.dev/blog/openclaw-vs-hermes-agent | third-party comparison by an integration vendor |
| OpenClaw repository and docs index (already used in upstream patterns) | https://github.com/openclaw/openclaw, https://docs.openclaw.ai/llms.txt | official |

## Comparison

| Area | JARVIS | Hermes Agent (Nous Research) | OpenClaw |
| --- | --- | --- | --- |
| Core | Rust daemon and CLI, SQLite | Python | Node.js gateway |
| Trust model | Deterministic policy for JARVIS-owned tool effects, durable approvals, audit; a model cannot approve itself | "Smart" command approval by default; sandboxed backends recommended | Trusted-operator assumption; sandboxing off by default; CVE-2026-25253 (token theft) and CVE-2026-32922 (admin-token minting) in 2026 |
| Memory | Owner-admitted memories, inspect/correct/forget, lexical task-aware recall, FTS5 over granted files | Always-loaded agent notes and user profile, all sessions in SQLite FTS5, eight external memory plugins | Editable Markdown memory, cross-session search |
| Learning | Proposed procedures the owner approves; offered when 75% of the "when to use it" words appear in the request ([skills](../user/skills.md)) | Creates and edits skills while working; agentskills.io format; experimental DSPy/GEPA self-evolution | Skill Workshop turns corrections into skills (`auto`, `propose`, `off`) |
| Channels | Browser console, CLI, Telegram | 27+ reported (Telegram, Discord, Slack, WhatsApp, Signal and others) | WhatsApp, iMessage, Slack, Discord, Telegram and 20+ others |
| Execution | Approved host processes ("not a sandbox"), Docker analysis, ACP coding agents | Local, Docker, SSH, Singularity, Modal, Daytona, Vercel Sandbox | Local, sandbox, paired nodes, cloud workers |
| Models | OpenAI-compatible endpoints only | Nous Portal, OpenRouter, any OpenAI-compatible endpoint, vLLM | Any, owner-supplied keys |
| Install | Source build with Rust; unsigned prereleases; ElevenLabs key required by guided setup ([ADR-0214](../adr/0214-first-run-tests-model-and-required-voice-before-saving.md)) | One-line installer, desktop app, hosted cloud | Installer, apps, one-click hosted deploys |
| Skills ecosystem | Local/public-commit SKILL.md bundles for coding tasks | agentskills.io, several registries | ClawHub marketplace (malicious-skill and update-poisoning risk listed in its threat model) |
| Licence | Apache-2.0 | MIT | MIT |

## Conclusions

JARVIS's lead is its authority boundary: deterministic policy, durable approvals and audit. The two rivals' 2026 incidents (OpenClaw CVEs and marketplace supply-chain risk) are what that boundary is designed to prevent, so P12 must not weaken it. JARVIS trails on install effort, learning, semantic memory, channel reach, execution isolation, native model adapters, media output and ecosystem. Each gap is a P12 item; where an existing ID already owns the work, P12 links to it and orders it rather than duplicating it.

The skill-matching and model-adapter findings come from this repository ([skills guide](../user/skills.md), [model provider research](integrations/model-providers.md): the Anthropic OpenAI-compatibility layer is "not considered a long-term or production-ready solution", ignores `strict` tool schemas, has no prompt caching and returns no thinking).
