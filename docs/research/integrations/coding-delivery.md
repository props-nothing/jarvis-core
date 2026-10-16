# Coding delivery contracts

Verified: 2026-10-16 UTC. Owner-requested coding workflow changes.

## Live delegation repair, 2026-10-16

Questions: why authorized dependency installs were offline, whether quiet ACP
turns imply failure, and how to retain useful progress and verification evidence.
Official sources fetched before implementation:

- https://agentclientprotocol.com/llms.txt
- https://agentclientprotocol.com/protocol/v1/prompt-turn
- https://agentclientprotocol.com/protocol/v1/tool-calls
- https://agentclientprotocol.com/protocol/v1/session-modes
- https://raw.githubusercontent.com/agentclientprotocol/codex-acp/v2.2.2/README.md
- https://raw.githubusercontent.com/agentclientprotocol/codex-acp/v2.2.2/src/AgentMode.ts
- https://raw.githubusercontent.com/agentclientprotocol/codex-acp/v2.2.2/src/CodexAcpClient.ts
- https://docs.npmjs.com/cli/v11/using-npm/config/
- https://docs.npmjs.com/cli/v11/commands/npm-install/
- https://raw.githubusercontent.com/microsoft/playwright-mcp/v0.0.83/README.md

Retain ACP v1, codex-acp 2.2.2, npm 11 and Playwright MCP 0.0.83. ACP v2
remains draft; no relevant v1 removal was observed. Partial tool updates retain
omitted fields; content-only updates need not produce another public progress
row. A quiet period is not an ended turn. Completion is the prompt response.

The pinned Codex adapter defaults to `agent` (auto review). Both `agent` and
`workspace-write` explicitly send `networkAccess: false` on every turn, so a
CODEX_CONFIG network override alone is ineffective. `INITIAL_AGENT_MODE` is
documented. `workspace-write` uses on-request approval with the user reviewer,
allowing approved command escalation to reach JARVIS's ACP permission policy.
Do not silently select `agent-full-access` or replace the owner's Codex config.
Select workspace-write for authorized commands and read-only otherwise, only
for the recognized Codex adapter. Explain dependency retries and project-local
cache use in the brief. npm configuration flags/environment override npmrc;
offline mode forbids fetching. Network permission is not network reachability.

Authentication remains with the configured agent; no account material is read
or copied. Other runtimes keep their configuration. Network-disabled work never
receives the network-retry instruction. General host execution remains the
existing approved capability, not a new sandbox guarantee. No new dependency,
automatic job retry, global install or provider API is introduced.

Verification: bounded waits must leave quiet jobs running and preserve Stop;
duplicate progress floods must retain terminal changes; oversized lists must
retain task IDs and statuses; zero-exit skipped lint and missing check scripts
must not pass delivery. A real disposable Codex/npm run is separate acceptance
from scripted-agent checks. Provider quotas, registry access and owner runtime
configuration may still block installs and must be reported explicitly.

Live Windows evidence (2026-10-16): the pinned signed-in Codex adapter completed
the disposable `is-number@7.0.0` installation smoke in 100 seconds; JARVIS ran a
separate Node import/behavior check successfully and verified the supplied
package/check files remained unchanged. No global settings or credentials were
changed. This verifies the tested npm installation path, not general website
delivery or other runtime/platform configurations.

## Versioned authority

- ACP index: https://agentclientprotocol.com/llms.txt
- ACP v1 sessions: https://agentclientprotocol.com/protocol/v1/session-setup
- ACP cancellation: https://agentclientprotocol.com/protocol/v1/cancellation
- ACP updates: https://agentclientprotocol.com/updates
- Skills index: https://agentskills.io/llms.txt
- Skills specification: https://agentskills.io/specification
- Git clone: https://git-scm.com/docs/git-clone
- npm install: https://docs.npmjs.com/cli/v11/commands/npm-install/

Indices and specifications were fetched during this task. ACP v2 is still
identified as draft. Retain v1 and existing pinned adapters; no new SDK.
The search reader rejected ACP Markdown pages; use the HTML counterparts.

## Contract and boundary

Public GitHub discovery uses GET /repos/{owner}/{repo}/commits/{ref}, then
GET /repos/{owner}/{repo}/git/trees/{tree_sha}?recursive=1. Installation requires
the discovered full commit SHA, never a moving branch. Files are downloaded by
immutable blob SHA using GET /repos/{owner}/{repo}/git/blobs/{sha} with
Accept: application/vnd.github.raw+json. No archive extraction, arbitrary
download URLs, redirects or credential forwarding. Public anonymous rate limits
and private repositories can block this path; report the HTTP status, do not
invent credentials or partially advertise a bundle as installed. Truncated
trees are refused rather than silently omitting resources.

Official contracts fetched 2026-10-15:
- https://docs.github.com/en/rest/commits/commits#get-a-commit
- https://docs.github.com/en/rest/git/trees
- https://docs.github.com/en/rest/git/blobs

Use API version 2026-03-10 as documented by the current official examples.
Modes 100644/100755 are regular files; reject symlinks (120000), submodules
(160000), unsafe Windows names and case-folding collisions. Preserve Unix
executable bits where supported. Apply per-file and total transfer limits.

YAML frontmatter is parsed by serde_yaml_ng =0.10.0 (MIT), confined to this
adapter. Official source and API fetched 2026-10-15:
- https://raw.githubusercontent.com/acatton/serde-yaml-ng/master/Cargo.toml
- https://docs.rs/serde_yaml_ng/0.10.0/serde_yaml_ng/
The implementation supports YAML 1.1; scalar names/descriptions must be strings.
No handwritten YAML parser or execution of bundle setup scripts is introduced.
Removal is isolated to the bundle adapter and its Cargo dependency.

ACP sessions use an absolute cwd. Overall job deadlines are local policy, not
a protocol requirement. Cancellation, handshake and write deadlines remain.
Skills contain YAML-frontmatter SKILL.md and optional resources. Instructions
do not supply missing dependencies, authentication or permissions. Reject
symlinks, traversal and destination overwrite when installing bundles.
Package installs are host code execution, including npm lifecycle scripts.
Use project-local environments and preserve lockfiles. Do not initialize Git,
commit, deploy or install globally without the requested task authorizing it.
Provider credentials remain with the configured agent. A cwd permission policy
is not an OS sandbox. No automatic provider credential sharing is introduced.

## Discriminating tests

Actual agent output in a non-Git granted folder; dirty files visible; absolute
outside-grant and symlink escapes refused; no omitted-deadline expiry while
cancellation still works; jobs queue and queued cancellation starts no process;
migration retains task activity; independent failed checks never look verified;
skill resources installed with unsafe bundles failing closed.

## Unresolved capabilities

Output buffers stay bounded independently of duration. Provider quotas, missing
runtimes and sign-in are explicit blockers. Record interrupted work on restart;
do not silently replay effects. ACP session loading and a JARVIS tool bridge
need negotiated support and scoped credentials; never pass the owner key.

## SQLite approval-column migration verification

The existing bundled SQLite 3.51.3 and SQLx 0.9.0 remain selected. Official ALTER TABLE documentation was fetched 2026-10-15: https://www.sqlite.org/lang_altertable.html. Rename/add/drop column are supported; dropping referenced columns is refused. No writable_schema or disabled foreign-key enforcement is used. The held arguments column has no external index/view/trigger dependency in the migration set; rename/add/copy/drop preserves its contents while applying a 1 MiB byte check. Task/activity table replacement explicitly saves/recreates the dependent activity table and indexes. The 28-to-29 regression retains real task/activity rows and checks PRAGMA foreign_key_check plus cross-workspace insertion refusal. Existing migration backup/fail-closed startup behavior remains unchanged.
