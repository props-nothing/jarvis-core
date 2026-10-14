---
integration: acp
status: researched
last_verified: 2026-10-14
owners: []
selected_spec_version: "ACP protocol v1 (stable); v2 is draft and is refused"
selected_sdk: "none: a small JARVIS client over JSON-RPC on stdio (see Decisions)"
---

# Agent Client Protocol (ACP)

## Scope

JARVIS acting as an ACP **client**: it starts a coding agent (Claude, Codex, Gemini, Copilot and others) as a child process on stdio, creates a session in a git worktree, sends one prompt, follows the streamed updates, answers the agent's permission requests by policy, can cancel, and reads the result. Operations: `initialize`, `session/new`, `session/prompt`, `session/update` (notification, received), `session/request_permission` (request, received), `session/cancel` (notification, sent).

Out of scope for this record and the first slice: JARVIS acting as an ACP **agent** (it already serves MCP, `ADR-0170`), `session/load` / `resume` / `list`, authentication methods (`authenticate`), the Client `fs/*` and `terminal/*` methods (not advertised), elicitation, session modes, Streamable HTTP transport (draft), protocol v2.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| `llms.txt` | https://agentclientprotocol.com/llms.txt | 2026-10-14 | discovery (lists v1 and v2 trees) |
| Overview, v1 | https://agentclientprotocol.com/protocol/v1/overview.md | 2026-10-13 | methods and roles |
| Initialization, v1 | https://agentclientprotocol.com/protocol/v1/initialization.md | 2026-10-13 | version and capability negotiation |
| Session setup, v1 | https://agentclientprotocol.com/protocol/v1/session-setup.md | 2026-10-13 | `session/new` |
| Prompt turn, v1 | https://agentclientprotocol.com/protocol/v1/prompt-turn.md | 2026-10-14 | turn lifecycle, updates, stop reasons |
| Tool calls, v1 | https://agentclientprotocol.com/protocol/v1/tool-calls.md | 2026-10-14 | partial tool updates, locations and permissions |
| Cancellation, v1 | https://agentclientprotocol.com/protocol/v1/cancellation.md | 2026-10-14 | optional request cancellation, `$/cancel_request`, `-32800` |
| Transports, v1 | https://agentclientprotocol.com/protocol/v1/transports.md | 2026-10-13 | stdio framing |
| JSON Schema, v1 | https://raw.githubusercontent.com/agentclientprotocol/agent-client-protocol/main/schema/v1/schema.json | 2026-10-14 | exact shapes; the v1 tree on main, not a pinned SDK |
| Updates | https://agentclientprotocol.com/updates.md | 2026-10-14 | v2 remains draft; optional tool names stabilized September 17, 2026 |
| Migration to v2 | https://agentclientprotocol.com/protocol/v2/migration.md | 2026-10-13 | what v2 changes and its draft status |
| Agents list | https://agentclientprotocol.com/get-started/agents.md | 2026-10-13 | which agents speak ACP |
| Registry entries | https://github.com/agentclientprotocol/registry (`claude-acp`, `codex-acp`, `gemini`, `github-copilot-cli` `agent.json`) | 2026-10-13 | how each agent is launched |
| Rust library | https://agentclientprotocol.com/libraries/rust.md, https://crates.io/api/v1/crates/agent-client-protocol | 2026-10-13 | SDK choice |
| changelog | not found as a page; versions are in the schema crate and registry | 2026-10-13 | n/a |
| Tokio Notify, pinned 1.53.1 | https://docs.rs/tokio/1.53.1/tokio/sync/struct.Notify.html | 2026-10-14 | single stored cancellation permit and waiter lifetime |
| Tokio select, pinned 1.53.1 | https://docs.rs/tokio/1.53.1/tokio/macro.select.html | 2026-10-14 | biased polling and cancellation of losing futures |
| Windows tasklist/taskkill | https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/tasklist, https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/taskkill | 2026-10-14 | PID-filtered process checks and `/T` descendant termination |
| POSIX kill, Issue 8 | https://pubs.opengroup.org/onlinepubs/9799919799/utilities/kill.html | 2026-10-14 | signal-zero process checks and process-group operands |

## Verified Contract

### Operations And Transport

- **JSON-RPC 2.0 over stdio.** The client launches the agent as a subprocess. Messages are single lines ending in `\n` with no embedded newlines, UTF-8. The agent MUST NOT write anything but ACP messages to stdout; it MAY log to stderr. The client MUST NOT write anything but ACP messages to the agent's stdin.
- **Handshake.** `initialize` with `protocolVersion` (integer, the latest the client supports), `clientCapabilities` (all optional; omitted means unsupported) and `clientInfo`. The agent answers with the same version if it supports it, otherwise **its own latest**; a client that does not support the answer SHOULD close the connection. The answer carries `agentCapabilities` (`loadSession`, `promptCapabilities`, `mcpCapabilities`, `sessionCapabilities`), `authMethods` and `agentInfo`.
- **`session/new`**: params `cwd` (absolute path) and `mcpServers` (array, required, may be empty; stdio servers must be supported by every agent). Result `sessionId`.
- **`session/prompt`**: params `sessionId`, `prompt` (array of content blocks; `text` is always allowed). The response arrives **when the turn ends** and carries `stopReason`: `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`. In between the agent sends `session/update` notifications whose `params.update.sessionUpdate` is one of `user_message_chunk`, `agent_message_chunk`, `agent_thought_chunk`, `tool_call`, `tool_call_update`, `plan`, `available_commands_update`, `current_mode_update`, `config_option_update`, `session_info_update`, `usage_update`, and (only with client capabilities) `notice`, `compaction_update`, `compaction_summary_chunk`. A client must ignore variants it does not know (the schema marks lenient decoding).
- **`session/request_permission`** (agent to client, a request): `sessionId`, `toolCall` (a `ToolCallUpdate`: `toolCallId`, optional `kind`, `status`, `title`, `content`, `locations`, `rawInput`, `rawOutput`) and `options` (`optionId`, `name`, `kind` of `allow_once`, `allow_always`, `reject_once`, `reject_always`). The response is `{"outcome": {"outcome": "selected", "optionId": …}}` or `{"outcome": {"outcome": "cancelled"}}`. A client MAY answer by policy. Tool kinds: `read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `switch_mode`, `other`.
- **Cancellation.** `session/cancel` (notification, `sessionId`). The client MUST answer every pending permission request with `cancelled`. The agent MUST then end the prompt with `stopReason: "cancelled"`. A separate `$/cancel_request` exists for single requests; error `-32800` means cancelled.
- **Errors.** `-32700` parse, `-32600` invalid request, `-32601` method not found, `-32602` invalid params, `-32603` internal, `-32800` cancelled, `-32000` authentication required, `-32002` resource not found.
- **Absolute paths** everywhere; line numbers 1-based.

### Authentication And Authorization

- ACP has no credential of its own for the transport (a child process on stdio). An agent that needs the owner's account lists `authMethods` in the `initialize` answer. In v1 a method is of kind `agent` (the default, handled by the agent through `authenticate`) or `terminal` (the client runs the agent interactively; offered only if the client advertises `auth.terminal`, which JARVIS does not). The agent signals that a sign-in is needed with error `-32000` on `session/new` **or later, on `session/prompt`**.
- **Implemented (2026-10-13, `ADR-0208`):** JARVIS reads `authMethods`, keeps the `agent` kind, and sends `authenticate {methodId}` (a) when the owner presses Sign in, whatever the agent has said, and (b) when the agent answers `-32000` and the owner has saved a method: once, at `session/new` or at the first prompt, then the request is repeated once. It stores no credential: the agent keeps its own.
- **Live evidence:** `@agentclientprotocol/codex-acp@2.2.2`, started through `npx -y` on Windows (Node 24), answered `initialize` with protocol version 1, `agentInfo` `@agentclientprotocol/codex-acp` 2.2.2 and two agent-kind methods, `api-key` ("Use an API key to authenticate") and `chat-gpt` ("Use ChatGPT to authenticate"), and **made a session without asking for a sign-in** while not signed in through JARVIS. So `session/new` succeeding does not mean the agent is signed in; the console says so. The handshake took about 10 seconds including the download. `authenticate` itself was **not** run (it signs in to a real account).
- The agent runs with whatever it is given. Containment is JARVIS's job (environment allowlist, working directory, worktree, permission policy), not the protocol's.

### Limits And Failure Semantics

- The protocol sets no size, time or concurrency limits. JARVIS sets them (line length, total output, turn time).
- A client that does not advertise `fs` and `terminal` capabilities will not receive those requests. An agent's *own* native tools (it edits files and runs commands itself) are not visible to the client at all except as `tool_call` updates and permission requests.
- Ambiguous outcomes: a crash or a hang mid-turn leaves the worktree in an unknown state. The result must say so and keep the worktree for inspection.

### Data And Compliance

What leaves JARVIS: the task text and whatever the agent reads in the worktree go to the agent's own model provider, under the agent's own terms. JARVIS passes no secrets to the child.

### Versions And Deprecations

- **v1 is the stable version in use.** The registry's agents (Claude `@agentclientprotocol/claude-agent-acp@0.90.0`, Codex `@agentclientprotocol/codex-acp@2.2.2`, Gemini `@google/gemini-cli@0.63.0 --acp`, Copilot `@github/copilot@1.0.96 --acp`) were read on 2026-10-13.
- **v2 is a draft** ("the v2 protocol surface as a whole is still labeled draft"). It removes `fs/*`, `terminal/*` and session modes, changes the prompt lifecycle (the `session/prompt` response only acknowledges; completion arrives as `state_update`) and renames methods. Implementers are told to negotiate per connection and keep v1.
- The official Rust crate `agent-client-protocol` is at 3.3.0 (2026-10-09, Apache-2.0, rust 1.88), depends on `agent-client-protocol-schema =1.11.0`, `futures`, `futures-concurrency`, `uuid` and others, and went from 1.3.0 (2026-07-20) to 3.3.0 in under three months.

## JARVIS Mapping

| ACP concept | JARVIS |
| --- | --- |
| the agent process | a supervised child of `jarvis-runtimes`; never loaded into `jarvisd` (`ADR-0006`) |
| `session/new.cwd` | a git worktree on a new branch, created by JARVIS, under the profile's `worktrees` folder |
| `session/request_permission` | decided by deterministic policy in Rust (the model cannot answer it); allowed only for kinds and paths the owner's one yes covered; everything else is rejected and recorded |
| streamed updates | counted and summarized into the tool's result; the last messages and the tool-call titles are kept, bounded |
| `stopReason` | `end_turn` = done; `cancelled`, `refusal`, `max_tokens`, `max_turn_requests` are reported by name |
| the tool | `jarvis.code.delegate`: risk 3, asks the owner once ("run this agent on this repository for this task"), result = summary, branch, diff stat and bounded diff; **nothing is merged or pushed** |

## Decisions

- **Speak v1 only.** Send `protocolVersion: 1`. If the agent answers any other version, refuse by name and close. Reason: v1 is what the agents speak today and v2 is draft.
- **Advertise no client capabilities** (`clientCapabilities` is `{}`: no `fs`, no `terminal`, no `session.*`). The agent then uses its own tools inside the worktree and asks permission through the one method every agent must support. Reason: the fewest client methods JARVIS has to police.
- **A small JARVIS client, not the SDK.** The surface used is six messages. The SDK's major version moved from 1 to 3 in three months, it pulls in `futures-concurrency` and a schema crate pinned to an exact version, and its types would have to be kept out of JARVIS domain types anyway (`AGENTS.md`). The client is about a few hundred lines over `serde_json` and `tokio`, which are already dependencies, and the fake agent proves its behaviour.
- **Tolerant reading, strict writing.** Unknown `sessionUpdate` variants and unknown fields are ignored; what JARVIS sends has exactly the required fields.
- **A stdout line that is not a JSON-RPC message** is counted, and the run is stopped when it passes a small limit: the spec forbids it, and an agent that does it is not speaking ACP.
- **Authentication through `authenticate`** for the agent kind of method only (see above), chosen by the owner, never automatic for a method the owner did not pick.

## Rejected Alternatives

- The `agent-client-protocol` crate: see above; revisit when v2 is stable and the crate's major version settles.
- Advertising `fs` and `terminal` so the client performs the agent's file and command work through the JARVIS tool pipeline: elegant, but v2 removes both, several agents prefer their native tools, and it would make JARVIS answer a stream of low-level requests. Containment by worktree plus a permission policy is simpler and honest about what it does and does not prevent.
- Running the agent inside the file tools' granted-folder sandbox: an agent's native tools do not go through JARVIS, so that sandbox cannot cover them.

## Verification Plan

- Offline, by a scripted fake agent (an example binary speaking newline-delimited JSON-RPC): handshake, version refusal, a prompt with streamed updates, a permission request allowed and rejected, a cancel, a crash, a hang, junk on stdout, an oversized line, an absent program.
- Policy tests: a permission request for a file outside the worktree, for a command, for a delete, with no options, with unknown kinds: each rejected.
- Falsification: a fake agent that writes outside its worktree. The first slice cannot **prevent** that (no OS sandbox) and says so; the test proves the run still ends in the worktree's results and that the parent repository is checked for change after the run (a changed parent repository is reported in the result).
- Cheapest test that would disprove the central assumption (that `{}` client capabilities plus `session/new` and `session/prompt` is enough to drive a real agent): one live run with the owner's installed agent. **Not yet run.**
- Live smoke test (opt-in): `npx @github/copilot --acp`, `npx @agentclientprotocol/codex-acp` or `gemini --acp` on a throwaway repository.

## Unresolved Questions

- Which agent the owner will use, and whether it is signed in (blocks: the live **task** run). The **handshake** has been run against a real agent (the Codex adapter); a task has not. None of `claude`, `codex`, `gemini`, `copilot` was installed as a command on the development machine on 2026-10-13 (only Node 24 and `npx`; the owner's OpenAI Codex desktop app is present but is not an ACP agent).
- Whether `authenticate` with `chat-gpt` completes in a headless daemon (it opens a browser page) and how long it takes (blocks: nothing; the sign-in has a five-minute limit).
- Exact `rawInput` and `locations` content per agent for edit requests (blocks: path checks on permission requests; the first slice treats an edit with no readable location as not provably inside the worktree and rejects it unless the owner allowed edits generally).
- Whether each agent honours `session/cancel` promptly (blocks: nothing; JARVIS kills the process after a grace period).
- Windows process-tree cleanup for agents that start their own children (blocks: "no orphan" on Windows beyond best effort).

## Live tool activity refresh, 2026-10-14

Scope: forward the existing v1 `session/update` tool activity while a prompt is running, rather than only returning it in the final report. No dependency, transport, sign-in, permission or cancellation-protocol change. The official index, prompt-turn guide, tool-call guide, v1 JSON Schema and updates page above were fetched during this task; no relevant deprecation of this v1 contract was observed.

Verified facts:

- A notification carries `params.sessionId` and `params.update`. Tool activity uses `tool_call` or `tool_call_update`; `toolCallId` identifies the call within that session. On updates, omitted fields retain their previous values. A status-only update must not lose the earlier title, kind or locations.
- Locations carry `path` and an optional unsigned line number. They are agent-reported metadata, not filesystem authority or proof that an operation happened.
- `session/prompt` responds when the turn ends. Tool updates may arrive before that response, including after cancellation was requested. The stream ends at the prompt response, not at an arbitrary pause in activity.
- `agent_thought_chunk` contains internal reasoning and is distinct from public tool activity. The progress path must not persist or display it. Raw tool input/output, tool content, prompts, authentication data and stderr are not needed for a progress summary.
- The protocol sets no delivery rate, queue size or retention limit, and agents need not report every action. Absence of updates is not proof that a process is idle.

JARVIS decisions and boundaries:

- Keep v1 and the existing SDK-free adapter. Normalize only bounded tool identifiers, titles, kinds, statuses and file locations for the active session. Unknown update variants and other sessions do not become progress.
- Use a bounded observer channel without letting a slow progress consumer block ACP reads, permission responses or cancellation. Progress is observation only and never changes authority, risk or tool outcomes.
- Persist retained activity in the local workspace before displaying it; bound retained history and disclose omissions. Display all agent-provided strings as inert data. A recorded tool status is the agent's claim, not independently verified success.
- No new credential, webhook, retry or network operation is introduced. Existing agent authentication, environment allowlist, time limits, data egress and ambiguous-effect rules remain unchanged. No automatic task replay is added.

Falsifying checks: receive a tool update while the prompt future is still unfinished; retain earlier metadata on a status-only update; exclude thought chunks, raw payloads and wrong-session notifications; flood a full observer channel and still cancel promptly; read persisted activity while the task remains running and again after reopening the database. A live run with a real ACP agent is separate evidence and is not claimed by these offline tests.

Earlier statements in this record that a real task has not run describe the original first-slice verification. `ADR-0209` subsequently records ordinary and background Codex tasks. That evidence does not verify the new progress delivery path.

## Startup cancellation refresh, 2026-10-14

Scope: make the existing owner Stop control effective before an agent's prompt begins (`P7-008`). The official index, v1 cancellation and prompt-turn guides, updates page, and pinned Tokio 1.53.1 references above were fetched during this task. ACP v1 remains selected; no relevant deprecation was observed. No dependency, credential, permission or wire-protocol change is planned.

Verified facts:

- `session/cancel` cancels an ongoing prompt turn and requires its session id. It cannot be relied on before session setup finishes. Generic `$/cancel_request` support and propagation to nested activities are optional, not process-termination guarantees.
- A Tokio `notify_one()` issued before its consumer starts stores one permit. The next polled `notified()` consumes it. Two simultaneous cancellation consumers can compete for that one permit.
- `select!` with `biased;` polls in declaration order. Losing futures are dropped; abandoning an in-flight setup must therefore terminate the child rather than continue using a partially initialized connection.
- Windows `taskkill /T` targets the selected process and its children. JARVIS must attempt tree termination while the setup future still owns the live parent; dropping that future first triggers `kill_on_drop` and can remove the parent before descendant cleanup. Native process queries are independent evidence, not an assertion that the kill helper returned successfully.

JARVIS decision: check an already-requested Stop before preparing or launching an agent; give cancellation priority during connection setup; keep active-prompt cancellation on the existing ACP path with only one consumer at each phase. Cancellation must remain a cancelled task, not a startup failure or a completed agent turn. Preserve any already-created worktree for review. Existing authentication, environment allowlist, effect/risk/scope declarations, privacy and non-retryable task semantics are unchanged. This does not provide OS confinement, parent-linked cancellation or crash resumption.

Falsifying checks: immediately start and stop a background task before yielding to its worker, then assert durable cancellation and no worktree; stop a scripted process during initialization, prove both its recorded PID and its child's PID were alive, and assert neither remains and the prompt's file write never happened. Include a normal task and existing active-turn cancellation as controls. Real-agent startup/sign-in cancellation and macOS/Linux process cleanup need separate live/native evidence and are not claimed by these tests.

## Verification Log

| Date | Versions checked | Relevant change or no-change evidence | Researcher |
| --- | --- | --- | --- |
| 2026-10-13 | ACP v1 docs and schema (main), v2 migration guide, registry entries listed above, crate 3.3.0 | first record; v2 draft; v1 selected | JARVIS agent |
| 2026-10-13 | v1 authentication page, codex-acp and claude-agent-acp READMEs, registry entries for opencode, goose, cursor, qwen-code, kimi, cline; live handshake with `@agentclientprotocol/codex-acp@2.2.2` | authentication implemented; presets for ten agents; the real Codex adapter completes the handshake through the JARVIS client (version 1, two sign-in methods, session made without a sign-in). The launch commands of the other agents were read from the registry and **not run** | JARVIS agent |
| 2026-10-14 | live official index, v1 prompt-turn and tool-call guides, v1 schema on main, updates page | partial tool updates and session scoping rechecked; v2 remains draft; public progress excludes internal reasoning and raw payloads; new progress delivery not yet verified live | JARVIS agent |
| 2026-10-14 | official ACP index, v1 cancellation/prompt-turn guides, updates; Tokio 1.53.1 Notify/select; Windows tasklist/taskkill and POSIX Issue 8 kill | startup cancellation contract refreshed; 18 delegation tests passed on Windows; deleting the setup tree kill left a child alive and failed the regression, restored guard passed; real-agent and macOS/Linux startup cleanup not verified | JARVIS agent |
