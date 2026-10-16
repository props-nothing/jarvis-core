# ADR-0230: Coding tasks are handed to an ACP agent running in its own git worktree, and the agent's permission requests are answered by policy

Status: Accepted
Date: 2026-10-13

**Identifier correction (2026-10-16):** this ACP coding delegation decision originally shared ADR-0207 with an unrelated phone decision. Its canonical identifier is ADR-0230; the original filename remains a historical alias. The original decision date and evidence below are retained.

**Amended by:** [ADR-0222](0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md) replaces mandatory worktrees, automatic commits and fixed total deadlines; [ADR-0224](0224-coding-waits-and-verification-preserve-real-task-outcomes.md) refines waiting and verification. Other original protocol/trust decisions remain.

## Context

JARVIS writes code itself with its file and command tools, which is slow and expensive for anything larger than a few edits. The owner has, or can have, coding agents of their own (GitHub Copilot, Gemini, Claude, Codex) that are built for exactly that. The Agent Client Protocol (ACP) is the open protocol editors use to start such an agent and follow what it does; every agent on its list speaks it, directly or through an adapter. `ADR-0006` says external runtimes are isolated adapters, and `ADR-0167` says open protocols come before bespoke code. Research is in `docs/research/integrations/acp.md` (read 2026-10-13).

An agent's own tools edit files and run programs without going through JARVIS, so JARVIS cannot put the agent behind its tool pipeline. What it can do is choose where the agent starts, what it is allowed to ask, what it inherits, how long it runs, and what is read back.

## Decision

1. **A new crate `jarvis-runtimes`** with two parts that know nothing of JARVIS's tools: `acp`, a client for ACP **version 1** over a child process's standard input and output, and `worktree`, git worktrees on a new branch. **No SDK:** the official Rust crate went from 1.3 to 3.3 in under three months and pins an exact schema crate; the six messages used are small, and the client is tested against a scripted fake agent. v2 of the protocol is a draft and is refused by number.
2. **The client advertises no capabilities.** No file or terminal methods are offered, so an agent cannot ask JARVIS to read, write or run anything; it uses its own tools in the folder it is given and asks leave through `session/request_permission`, which every agent supports. A request for any other method is answered "method not found".
3. **Permission requests are answered by deterministic Rust (`WorktreePolicy`), never by a model.** Reading and searching are allowed for paths that resolve inside the worktree (links are resolved first); editing, deleting and moving likewise, and an edit that names no path is refused; commands only when the owner's approval of this task said so; network fetches and every other kind are refused. Only a single allowance (`allow_once`) is ever chosen, never `allow_always`, which would remember the choice. Every request and its answer is returned in the result.
4. **One new tool, `jarvis.code.delegate`.** Effects `write` and `code_execution`, risk 3, `Ask`: the owner is asked every time and sees the repository, the task, whether commands are allowed and the time limit (the approval is the whole authority of the task, in line with `ADR-0136`: one yes, no steps added). The result is a summary, the diff and the branch name. **Nothing is merged, pushed or deleted.** A branch `jarvis/<task>-<id>` is made in a new worktree under the profile's `worktrees` folder, from the repository's last commit (uncommitted changes are not in it, and the result says so). What the agent changed is committed on that branch with hooks off, so the branch is complete.
5. **The agent starts clean.** The environment is the same small allowlist a build command gets (`ADR-0143`), plus the variable *names* the owner lists in `code_agent_env`; no JARVIS variable and no token otherwise. Settings: `code_agent` (the command, an argument vector; unset means JARVIS looks for an installed `copilot` or `gemini`) and `code_agent_env`.
6. **Bounded.** A time limit (5 minutes unless asked, 7 at most: a tool call may not outlast the pipeline's 600 seconds, which also covers starting the agent), then `session/cancel`, a ten-second grace period and the whole process tree is killed; the same happens when the run is cancelled. One coding task at a time. Lines over 2 MiB, more than 20 lines that are not JSON, and an agent that stops reading its input each end the task with a named reason.
7. **What comes back is untrusted** and is fenced piece by piece: the agent's words, its tool titles, the requests it made, the file names and the patch. The adapter's own statements (outcome, branch, counts, notes) are not fenced and no agent text is ever placed in them.
8. **Honest about the limit.** The worktree and the policy contain a well-behaved agent and make a misbehaving one visible; they are **not** an operating-system sandbox. An agent's native tools can still reach the rest of the owner's disk. The original repository is compared before and after the run and a difference is reported as "the agent wrote outside its worktree".

## Consequences

- JARVIS can supervise Copilot, Gemini and (through their adapters) Claude and Codex with no per-agent code, and a new ACP agent needs only a `code_agent` line.
- Nothing is run without the owner's yes, and nothing leaves the machine except to the agent's own provider under the agent's own terms.
- The first run of an agent started through a package runner downloads it, which can exceed the 90-second start limit; the refusal says so and the agent should be run once in a terminal.
- Live evidence 2026-10-13: the real Codex ACP adapter (using the owner's existing ChatGPT/Codex sign-in) completed a task in a throwaway repository, changed two files on branch `jarvis/in-this-python-repository-make-ad328550`, and committed them; the original checkout stayed on `main` and clean. Its requested Python check could not run because `python` was not on the agent's clean search path, and that failure was returned honestly. A second task completed through the background form (`ADR-0209`) on branch `jarvis/in-greet-py-add-a-function-exc-e5ba4dbc` while the start call returned immediately.
- Not done, and recorded in `TODO.md`: a container for the agent where Docker exists, push/Telegram completion, task cleanup, and broader live runs against agents other than Codex.
- An agent that needs a client-provided MCP server (JARVIS's own tools for the agent, `P11-001e`) is not offered here: no MCP server is passed in `session/new`.
