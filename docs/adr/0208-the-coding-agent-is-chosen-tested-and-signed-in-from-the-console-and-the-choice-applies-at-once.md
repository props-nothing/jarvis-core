# ADR-0208: The coding agent is chosen, tested and signed in from the console, from a short list of agents, and the choice applies at once

Status: Accepted
Date: 2026-10-13

## Context

`ADR-0230` hands coding tasks to an ACP agent, but choosing one meant `jarvis config set code_agent npx -y …` in a terminal and then a restart, with a test only by running a real task. The owner asked for the setup to be easy from the console, and for more agents than Copilot and Gemini, ChatGPT (Codex) among them. Reading the Codex adapter showed that agents need more than a launch command: the adapter offers two sign-in methods (`api-key`, `chat-gpt`) through ACP's `authenticate`, and it made a session without asking for one, so a handshake that succeeds does not show that the agent is usable.

## Decision

1. **Presets.** `code_agents.rs` lists ten agents with the launch commands in the ACP registry (read 2026-10-13): ChatGPT (Codex), Claude, Copilot, Gemini, OpenCode, goose, Cursor, Qwen Code, Kimi and Cline. Each is started as the program the owner installed when that is on the search path, otherwise through `npx -y` with the package **pinned to the registry's exact version** (what is downloaded is what was read), otherwise it is shown as not found. An adapter downloaded by `npx` is never started unless the owner chose it; automatic detection (nothing chosen) only ever starts an installed `copilot` or `gemini`.
2. **The console's Folders & code tab gets a Coding agent card** (`GET /api/v1/code-agents`, `POST …/test`, `POST …/sign-in`): a checklist (folder granted, git installed, agent chosen), one row per preset with whether it can be started on this computer, a Use button, the names of environment variables the agent may inherit, a command field for any other ACP agent, and Test. Saving applies **at once**: the delegate tool reads the setting when a task starts, not at startup.
3. **Test** starts the saved agent in an empty temporary folder, completes the handshake, closes it, removes the folder and says who answered, in a sentence. It runs no task and touches no repository. When the agent offers sign-in methods and has not said it needs one, the answer says it cannot tell whether the owner is signed in, and the card shows one button per method.
4. **Sign in** is the owner's click on one of the agent's own methods. JARVIS sends `authenticate` with it (the agent runs its own flow, which may open a browser page; five minutes at most) and, if it worked, saves the method as `code_agent_auth`. A task then uses that method **only if the agent says it needs a sign-in**, whether when the session is made or at the first prompt, and repeats the request once. JARVIS stores no credential. Only the kind of method the agent handles itself is offered; the terminal kind is not, because JARVIS does not advertise it.
5. **One check at a time**, and the same environment allowlist as a task: the agent gets the safe set and the variable names the owner listed.

## Consequences

- Choosing and testing an agent is a few clicks, and a wrong choice (not installed, not signed in, wrong version) is found by Test instead of by a failed task.
- The Test was run live on 2026-10-13 against the real Codex adapter (`@agentclientprotocol/codex-acp@2.2.2` through `npx`): it answered, offered the two methods and made a session unsigned, which is why decision 3 exists. The Sign in click and a **task** against a real agent have not been run.
- The pinned `npx` versions go stale; updating them is an edit to the preset table after reading the registry again. The launch commands of the agents other than Codex were read, not run.
- Not done: a list of delegated tasks in the console, the asynchronous form for tasks over seven minutes, and a prerequisite check beyond finding the program (for example that Node is new enough for an adapter).
