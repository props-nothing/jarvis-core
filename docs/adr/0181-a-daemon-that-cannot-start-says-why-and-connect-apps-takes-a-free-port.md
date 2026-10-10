# ADR-0181: A daemon that cannot start says why, and Connect apps takes a free port

Status: Accepted
Date: 2026-10-10

## Context

A browser test of the shipped build on a fresh profile found that the console's "Restart now" left a dead console. The new profile had taken Connect apps' default port (8766), which another JARVIS on the same computer already held, so the restarted daemon refused to start. Two things made that worse than it had to be: the port was chosen without looking, and `jarvis start` (which the console's restart runs) said only "the daemon exited at startup. Run `jarvis daemon` in a terminal to see why", so the console showed nothing at all.

## Decision

1. **Switching Connect apps on takes the first free port** from 8766 (up to thirty tried), checked by binding to it. A port the owner set by hand is left as set.
2. **A daemon that cannot start records why.** On a startup failure it writes the error and its causes on one line to `startup-error.txt` in its runtime directory (best effort, never hiding the original error). `jarvis start` removes a stale file before starting and, if the daemon exits, prints the reason: "the daemon could not start: the MCP endpoint could not be served: port 8766 could not be used…". The terminal fallback remains when no reason was left.

## Consequences

Checked in a real browser on a fresh profile with another daemon holding 8766: Switch on chose 8767, the console's Restart now brought the daemon back, and Connect apps showed on. Checked by hand: with 8766 forced on the profile, `jarvis start` printed the port clash by name. A unit test covers skipping a held port. Not done: the console does not yet show a start failure itself (it can only reconnect after a restart); `jarvis restart` from the console that fails leaves the console unreachable and the reason is in the file and the terminal output.
