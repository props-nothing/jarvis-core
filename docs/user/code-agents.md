# Coding agents: build in the folders you permit

[Documentation](../README.md) / [Settings](settings.md) / [Skills](skills.md) / [Security](../../SECURITY.md)

JARVIS hands substantial coding work to your selected ACP agent. The default working directory is **the first folder permitted in Settings, Folders & code**. It is not the daemon source directory, profile directory or a generated checkout. Reorder/change your permitted folders to change that default.

## Workspace and Git

- Omit the task's repository field, or use a dot, to select the first permitted folder.
- An existing absolute path selects a folder inside any permitted root. Relative paths resolve within those roots; an ambiguous match is refused instead of guessed.
- The default mode is in_place. Existing and uncommitted files are visible. A non-Git folder is valid. JARVIS does not initialize Git, stage files, create a branch or commit the result in this mode.
- Explicit mode worktree opts into a separate Git checkout and branch under the profile. That requires Git and a committed repository. JARVIS reviews tracked changes and lists untracked paths, but does not automatically stage, commit, merge or push. New untracked file contents are not included in the tracked diff.
- An agent's own native tools can still invoke Git. The task brief instructs it not to do so unless requested; this is not an OS-enforced prohibition.

## Set up your agent

1. Grant your project folder in **Settings, Folders & code**. Git is optional for normal in-place work.
2. Pick an installed agent or a pinned downloaded ACP adapter, or enter your own ACP command. Downloaded Node adapters need Node and registry access.
3. Press **Test** for an empty-directory handshake. This does not run your coding task.
4. Use **Sign in** if the agent exposes a method, or sign in with its CLI. Agent authentication remains with that agent. JARVIS does not copy editor credentials or account tokens into it.

Agent selection applies without restarting JARVIS. Tool availability and permitted-folder changes follow the existing Settings lifecycle; restart if the console says one is required.

## Durable jobs, not a seven-minute call

Both jarvis.code.delegate and jarvis.code.delegate_start return a durable task ID immediately. Jobs queue behind the active coding job rather than being refused as busy. Execution is serialized to avoid concurrent edits to the same files.

**There is no overall job deadline by default.** Omit minutes or use zero. An explicit positive value is an owner-selected deadline, not a product-imposed maximum. Protocol handshakes, cancellation grace, individual HTTP requests and independent verification commands remain bounded so a broken operation can fail instead of hanging forever.

Use jarvis.code.delegate_wait with the task ID to wait up to 30 seconds per call. Repeat while the job is queued/running; a wait timeout or quiet progress does not cancel it or mean failure. A failed individual setup command gives the agent a chance to recover. Use delegate_status for an immediate snapshot, delegate_list for compact identities and delegate_stop when cancellation is intended. A queued Stop starts no agent. Startup Stop terminates the agent and its child tree; active prompts receive cancellation and termination. Files remain available for review.

After daemon restart, unfinished jobs become interrupted, not silently replayed. jarvis.code.delegate_resume asks for approval and starts a **new** in-place agent using the saved directory, brief, capabilities and checks. It first inspects existing files and avoids replaying completed effects. It does not restore an ACP conversation, rerun a previous process automatically or resume legacy worktree tasks blindly. Provider limits and sign-in can still interrupt a job.

An ordinary stop or Settings restart refuses while native runs, queued/running coding jobs or managed preview processes would be interrupted, even after the parent conversation has finished. The reply lists the counts. An unreadable database or process registry also leaves the daemon running and reports that work state is unavailable. Wait for work to finish or explicitly use `jarvis stop|restart --force` to interrupt it; native runs parked for an owner approval remain durable and do not block a restart. This is a preflight check, not an atomic freeze on new work arriving during shutdown. See [ADR-0232](../adr/0232-stop-and-restart-check-background-work-and-fail-closed.md).

## Skills, dependencies and integrations are different

Use jarvis.skills.discover to list project bundles from .agents/skills, .github/skills and .claude/skills. Optional public GitHub discovery resolves a branch/tag to an immutable commit and lists manifest paths. Installation uses that full commit SHA and selected bundle path, not a moving branch.

jarvis.skills.install copies the complete bundle, including references, scripts and assets, into the selected project's .agents/skills/name. Local sources must also be in permitted folders. Existing bundles are not overwritten. Traversal, links/reparse points, unsafe Windows names, collisions and excessive resource trees fail closed. Provenance is saved with the bundle. Installation executes **no scripts** and grants **no permissions**.

Select installed skill names in the coding task's skills array. The agent receives available metadata and the full manifests of selected skills with their resource directories. A missing or ambiguous selected skill prevents the agent from starting. Native jarvis.skill learned procedures are a separate feature, not filesystem bundles.

A bundle cannot install its own runtime, obtain credentials or connect a service merely by declaring them. Task-authorized project-local dependency setup is host execution. Preserve lockfiles, use the project's package manager and environment, and do not install globally or deploy without authorization. npm lifecycle scripts and downloaded executables are code, not harmless configuration.

Public GitHub installation uses anonymous HTTPS without redirects or forwarded credentials. Private repositories, rate limits, unavailable runtimes and registry/network errors remain explicit blockers. Use a permitted local source for a private bundle obtained through your own authorized workflow. Global skill catalogs, automatic plugin installation and an agent-scoped JARVIS integration bridge are not implemented by this path.

## Independent checks and website delivery

Give the agent the complete brief, design direction, functionality, preservation requirements and acceptance checks. A queued job or a confident agent summary is **not** a delivered website.

JARVIS runs the task's independent checks after the agent finishes. Without explicit checks, an allowed-command task detects package.json build, lint and typecheck scripts and selects npm, pnpm, yarn or bun from packageManager/lockfiles. A failed check makes the task failed with verification_failed. No applicable checks means not_run, never passed. Delivery is separately labeled checks_passed, verification_failed or unverified; the console labels finished jobs without passing checks as unverified. Only the listed commands are covered. Passing a build does not establish visual or behavioral correctness.

An exit-zero command is insufficient when capture is incomplete or the build explicitly reports missing ESLint, skipped linting or skipped type validation. Such diagnostics are returned in reported_check_failures without changing the actual exit code. Advertised direct Node check scripts that do not exist fail verification. Complex shell scripts and other tools still require inspecting their actual results.

The pinned Codex preset uses workspace-write mode with permission requests routed to JARVIS. Authorized npm dependency setup overrides inherited offline mode and can retry with `--offline=false --cache .jarvis-cache/npm`; sandbox-blocked commands can request runtime permission. This does not grant full-access mode, change global configuration, or guarantee registry availability. Denied permission, sign-in and network failures remain explicit blockers.

For a dev server use jarvis.process.start with argv, directory and optional port. Configure the actual server to bind **127.0.0.1**. It returns a managed process ID without a lifetime deadline. jarvis.process.status exposes bounded, untrusted log tails and loopback reachability; jarvis.process.list and jarvis.process.stop manage its lifecycle. A port already in use is refused so an existing unrelated service is not presented as this preview.

The status explicitly says browser_verified: false. JARVIS must use its browser tools to inspect the real page at desktop/mobile sizes, exercise important interactions and fix failures before claiming website delivery. Browser verification is not automatically performed by the delegation adapter, and it cannot be inferred from a TCP port or build exit code. Save screenshots to absolute paths inside the selected project; relative screenshot filenames are refused because the browser server has a different working directory. Omitting filename is allowed for temporary browser artifacts.

Processes are not automatically restarted. Completed state/logs survive a daemon restart; a stale unfinished record is interrupted and never killed using a reused PID. Logs are capped at 16 MiB per stream and tails are bounded; capture loss is disclosed. Stop a preview when it is no longer needed.

## Review and trust boundaries

**Settings, Folders & code, Background coding tasks** shows the actual working folder, mode, deadline, agent activity and independent check evidence. Activity is agent-reported, bounded and separate from results. Refresh is read-only and preserves open reviews.

Approvals still apply to host coding jobs, dependency installation through approved commands, remote bundle installation and managed process starts. An approval authorizes the displayed task, not unrelated installs, deployment or account access. Full coding briefs are retained for approval and recovery; oversized reports remain explicitly incomplete, valid JSON.

These are **host processes, not a sandbox**. Directory checks set the initial cwd and cooperative ACP permissions. Agents and approved packages can access the host according to the OS privileges of the owner. JARVIS does not claim to intercept every native agent tool, enforce network isolation or prevent all filesystem races. Missing credentials, external quotas and provider capabilities cannot be bypassed. A scoped bridge for JARVIS-owned MCP/browser tools and native macOS/Linux acceptance remain separate work.
