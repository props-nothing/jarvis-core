# ADR-0180: A call from another app that needs the owner waits for the owner

Status: Accepted (amends `ADR-0039`; amended by `ADR-0200`: a yes spoken on a live phone call JARVIS placed can answer the question)
Date: 2026-10-10

## Context

`ADR-0039` refused any call from another app (Claude Code, a script, a phone agent) whose tool needed the owner's yes, because a remote call has no run to park: the database refuses a tool call without one, and the call is a live request with a caller waiting. The owner's intent for Connect apps is that an outside agent can use what JARVIS can do on this machine ("execute all"), and with every acting tool refused it could only read. Tools set to run without asking already ran for it; the gap was the tools that ask.

## Decision

1. **The call waits, and the owner answers.** A call whose decision needs the owner (the workspace's rule or the tool's own declaration) is no longer refused. The runner that holds the caller's request puts it in an in-memory list (`inbound_approvals`), shown first in the console's approvals list with the tool, risk and the exact arguments, and waits. One click, `jarvis approvals approve|deny ID`, or a spoken yes or no answers it through the same route as any approval.
2. **Fail closed, in four ways.** A denial refuses the call. No answer within two minutes refuses it and withdraws the question. More than eight waiting refuses the ninth at once. Anything the owner did not say yes to never runs. The yes names this call: the arguments shown are the arguments held and run.
3. **Same policy, same receipt.** `call_remote_tool_with` runs the same validation and decision; a denial still refuses; and the receipt **cites the owner's answer** (approval id, the profile's local identity, time), so an adapter sees the same shape of authority a local approved call carries. A call that needs no approval ignores it.
4. **In memory on purpose.** The caller's request does not survive a restart, so a durable approval would be a question nobody is waiting on. A restart drops waiting calls and the callers see a refusal.
5. **Unchanged.** The caller still cannot answer for itself (the answer route needs the console credential). The access key, loopback or tunnel, the rate bound and the tool list served are as before.

## Consequences

Tested: the registry (listed, answered once, a denial, the cap, an unknown id); the runner (waits and runs only after a yes with the adapter called once and not before; a no, silence and a full list each refuse and the adapter is never called). Run live on a throwaway profile through the real HTTP endpoint: an outside client called `jarvis.schedule.add`; the call **waited**; `jarvis approvals list` showed it with its arguments; `jarvis approvals approve` released it and the schedule was created; a second call answered with `deny` was refused with "the owner said no" and no schedule was created. A tool with its own limits still applies them: `jarvis.project.create`, approved, refused because an outside agent has no conversation to put the project in.

Checked live in the console: the waiting call appears under "needs you" with its risk, tool and exact arguments, and Approve released it ("Approved jarvis.schedule.add. Carrying on."). **A finding:** when the outside caller disconnects while waiting, the server keeps the call running, so the question stays listed until the two-minute limit withdraws it; approving it in that time still runs what the owner saw, and the caller never receives the result. A guard withdraws the question at once if the task holding it is cancelled. The cap of eight bounds what a key holder can leave behind.

Not done: remote calls are still not in the durable audit trail (they are logged, with the tool and the decision, not the arguments); one key shared by every outside agent, so the owner cannot tell which agent asked (a key per agent is the rest of `P11-001e`); a spoken or pushed alert when a call starts waiting (the console shows it within its polling interval, and the ntfy push covers stored approvals only); and a per-tool setting for how long to wait.
