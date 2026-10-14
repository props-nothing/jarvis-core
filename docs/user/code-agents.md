# Coding agents: hand JARVIS a coding task

[Documentation](../README.md) / [Quick start](quick-start.md) / [Settings](settings.md) / [Security](../../SECURITY.md)

> **Experimental first slice.** Scripted-agent tests, the real Codex adapter handshake and ordinary/background Codex tasks have recorded verification; the console sign-in flow on a real account remains unverified. Worktrees separate checkouts, not operating-system permissions. JARVIS does not automatically merge or push the result.

For a change bigger than a few edits, JARVIS can hand the task to a coding agent of your own (ChatGPT/Codex, Claude, GitHub Copilot, Gemini, OpenCode, goose, Cursor, Qwen Code, Kimi, Cline, or any other that speaks the Agent Client Protocol) instead of writing it all itself. Ask in plain words: "have the coding agent add a dark mode toggle to the `webshop` repository". JARVIS asks you first, then waits for the agent and brings back a summary, the diff and the name of a branch.

**Nothing is merged, pushed or deleted.** The agent works in a *new worktree on a new branch* (`jarvis/<task>-<id>`), a separate checkout of your repository under JARVIS's own folder, so your own checkout is not where it works. You read the branch and decide.

## Set it up (in the console)

Open **Settings, Folders & code**. The **Coding agent** card walks you through it:

1. **A folder** that holds your repository must be granted (the Folders card above it). The repository must be a git repository with at least one commit, and you name its own folder, not a folder inside it. **git** must be installed; the card ticks both off.
2. **Pick an agent** with **Use**. Each row says whether it is installed on this computer, or will be downloaded with `npx` the first time (this needs Node), or is not found. The `npx` versions are pinned to the ones in the ACP registry. Anything else that speaks ACP: open **Another agent (a command)** and type the command that starts it in ACP mode.
3. **Press Test.** JARVIS starts the agent in an empty folder, shakes hands with it and tells you who answered. It runs no task and touches none of your files. The first start through `npx` downloads the agent, which can take a minute.
4. **Sign in, if the agent offers it.** Agents keep their own sign-in. If the Test lists methods (for ChatGPT/Codex: **ChatGPT** or **API Key**), press the one you want: the agent runs its own sign-in, which may open a page in your browser (JARVIS waits up to 5 minutes), and JARVIS remembers the method so it can sign in again if the agent asks later. JARVIS never sees or stores your password or key. Or sign in once in a terminal with the agent's own command (the card says which).

Changes apply **at once**, with no restart. **Choose automatically** forgets your choice; JARVIS then uses an installed `copilot` or `gemini` if it finds one, and never starts a downloaded adapter you did not pick. The command and sign-in settings are `code_agent` and `code_agent_auth` for `jarvis config`.

The console uses the agent's normal sign-in, not an environment-variable editor. Advanced CLI configurations can still use `code_agent_env` to name existing environment variables the agent may inherit; saved configurations are preserved.

## What you are asked, and what the agent may do

JARVIS asks before every task and shows the repository, the task, whether commands are allowed and the time limit. Your yes covers that task and nothing else. (In Settings, Permissions you can mark the tool "run without asking" for a repository you trust.)

While it works, the agent asks leave for things, and JARVIS answers by fixed rules, never by a model:

| The agent asks to | JARVIS answers |
| --- | --- |
| read or search files | yes, inside the worktree only |
| edit, delete or move files | yes, inside the worktree only (a link that points out does not count; a request that names no path is refused) |
| run a command (a build, the tests) | only if the task was started with commands allowed |
| fetch from the network, or anything else | no |

JARVIS only ever says "yes, this once", never "always". Everything the agent asked, and the answer, is in the result.

## What comes back

The outcome (`completed`, `stopped`, `cancelled`, `timed_out`, `agent_exited` or `failed`), the branch name, the commit, the files changed, a diffstat, the start of the diff, what the agent said, the tools it used and the requests it made. The agent's words and the diff are data from another program, and JARVIS treats them as data, never as instructions.

To review: `git diff <base>..<branch>` in the repository (JARVIS gives the exact line). To keep the work, merge the branch yourself or ask JARVIS to ask the agent for changes. To throw it away: `git worktree remove <folder>` and `git branch -D <branch>`. Worktrees are kept until you remove them.

For background tasks, open **Settings, Folders & code, Background coding tasks** and expand a task. The review shows its recorded branch, worktree, commits, changed files, agent report, activity, permission decisions and diff preview. A truncated preview is labelled. **Refresh coding tasks** reloads status without running an agent or command, and keeps expanded tasks open. Failed refreshes keep the last loaded review visible and show an error.

**Completed means the agent ended its turn, not that tests passed.** Reviews say **Not independently verified**, even when the agent's own report says tests passed. Interrupted tasks explicitly report a missing final result; an early interruption may leave no recorded worktree path. This view does not resume, merge, push or independently test the work.

## Limits, said plainly

- **This is not a sandbox.** The agent's own tools run on your computer with your privileges and JARVIS does not see them. The worktree and the rules above contain a well-behaved agent and make a misbehaving one visible; they do not stop it. JARVIS compares your own checkout before and after, and says so if the agent wrote there.
- One task at a time. The ordinary `jarvis.code.delegate` waits for at most **7 minutes** (5 unless you ask for more). For longer work JARVIS uses `jarvis.code.delegate_start`: it asks once, returns a task id immediately, works for up to 60 minutes, notifies you when it ends, and keeps durable status. Ask it to check, list or stop a background coding task; the Coding agent card also lists recent tasks. A daemon restart marks unfinished work **interrupted** and never silently repeats it; its worktree is kept.
- The worktree starts from the last commit: uncommitted changes in your checkout are not in it (the result says when you have some).
- Tasks are tested against a scripted agent, and **two tasks were run live against the real Codex adapter** using the owner's existing ChatGPT/Codex sign-in (one ordinary, one background). Both made the requested two-file change on their own branch while `main` stayed clean. The console's explicit Sign in click has not been run on a real account because Codex already found that existing sign-in.
