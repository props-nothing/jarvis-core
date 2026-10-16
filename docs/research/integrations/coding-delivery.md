# Coding delivery contracts

Verified: 2026-10-15 UTC. Owner-requested coding workflow changes.

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
