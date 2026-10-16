# Skills: how JARVIS learns your routines

**A procedure it can learn. Never a permission it can grant itself.**

[Documentation](../README.md) / [Quick start](quick-start.md) / [Projects](projects.md) / [Permissions](settings.md)

A skill is a procedure: when to use it, and the tools to call in order. After a task that took many steps and that you are likely to ask for again, JARVIS can write one down. It is only a **proposal**. It changes nothing until you approve it.

## What you see

A card in the console, under what is waiting for you:

> **learn this?** Research a Dutch company and summarize it.
> 1. jarvis.web.fetch: Fetch a search results page for the company name…
> 2. jarvis.contacts.search: See whether the company is already known…

**Approve** and later requests like it are given the procedure. **Dismiss** and it is kept for the record, never used. Nothing else is needed: no code, no id.

## How a skill is used

When you ask for something, JARVIS offers the model every approved skill whose first line (the "when to use it" line) is covered by your request: at least three quarters of its distinctive words, and at least two. The model sees the procedure as data, not as an order, and every step still goes through the usual permissions and approvals. A skill gives JARVIS no new power.

You can try it: approve a skill, then ask for the same kind of thing in different words.

## Limits that keep it safe

- Only JARVIS's own tool `jarvis.skill.propose` writes proposals, and only as *proposed*. Approval is always yours.
- A step can only name a tool JARVIS actually has.
- At most ten proposals wait at once; the same proposal twice is shown once.
- Skills made from web pages or documents are no different: they still wait for you.

## From a terminal

`jarvis skills list` shows every skill and its state; `jarvis skills show ID` shows the text. To approve or dismiss without the console, use `jarvis skills promote` and `jarvis skills disable` (see `jarvis skills --help`).

## Coding-agent bundles are a separate capability

A filesystem Agent Skills bundle has a SKILL.md manifest and may include scripts, references and assets. Use jarvis.skills.discover and jarvis.skills.install for complete project-local bundles, then select their names in a coding task. These tools are not jarvis.skill.propose and do not create or promote learned SQLite procedures.

Bundles do not grant permissions, execute their setup scripts, install runtime dependencies or connect accounts automatically. See [coding-agent setup and blockers](code-agents.md#skills-dependencies-and-integrations-are-different) for local/public-GitHub installation, activation, project package setup and trust boundaries.
