//! Supervised coding-agent runtimes (`P7`, `ADR-0006`, `ADR-0207`).
//!
//! * [`acp`]: a client for the Agent Client Protocol, version 1, over a child process's standard input and output. It knows the
//!   protocol and nothing about JARVIS: it is given a started child, a working directory and a policy for the agent's permission
//!   requests, and it reports what happened.
//! * [`worktree`]: a git worktree on a new branch for an agent to work in, and the summary and diff of what it did.
//!
//! The daemon owns everything around them: which program is started and with what environment, which folders may be used, whether
//! the owner said yes, and what is kept. Provider types do not cross this boundary.

pub mod acp;
pub mod worktree;
