//! Ownership of authorized host processes and their reader tasks.
//!
//! This is lifetime management, not a filesystem, network or resource sandbox.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::process::ExitStatus;
use std::task::{Context, Poll};

use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::task::{JoinError, JoinHandle};

/// A root process and the lifetime of its ordinary descendants.
///
/// Windows children join a kill-on-close job before their first instruction. Unix
/// children lead a process group; children that deliberately leave it and abrupt
/// daemon death are outside that guarantee. No PID is restored from disk.
pub struct OwnedProcess {
    child: Box<dyn ChildWrapper>,
    terminate_on_drop: bool,
    termination_error: Option<String>,
}

impl OwnedProcess {
    /// Spawns a prepared command with process-tree ownership and no console window.
    ///
    /// # Errors
    /// Returns an error if spawning or establishing ownership fails. A Windows
    /// child is not resumed if job assignment fails.
    pub fn spawn(command: Command) -> io::Result<Self> {
        Self::spawn_wrapped(Self::wrap(command))
    }

    fn wrap(command: Command) -> CommandWrap {
        let mut command = CommandWrap::from(command);
        command.wrap(KillOnDrop);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        #[cfg(windows)]
        {
            command.wrap(process_wrap::tokio::CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ));
            command.wrap(process_wrap::tokio::JobObject);
        }
        command
    }

    fn spawn_wrapped(mut command: CommandWrap) -> io::Result<Self> {
        Ok(Self {
            child: command.spawn()?,
            terminate_on_drop: true,
            termination_error: None,
        })
    }

    /// Adopts a native child for the existing ACP connection API. This owns only
    /// the supplied child; production tree ownership must use [`Self::spawn`].
    pub(crate) fn from_child(child: Child) -> Self {
        Self {
            child: Box::new(child),
            terminate_on_drop: true,
            termination_error: None,
        }
    }

    /// The root's process identifier while it is running.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Takes the child's piped standard input.
    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin().take()
    }

    /// Takes the child's piped standard output.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout().take()
    }

    /// Takes the child's piped standard error.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr().take()
    }

    /// Waits for the root and terminates any descendants it leaves behind.
    ///
    /// # Errors
    /// Returns a wait or termination error; failed cleanup is not a completed command.
    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        // Wrapper wait() can wait indefinitely for descendants. The native wait
        // preserves command semantics; termination still uses the owned job/group.
        // Our constructor installs exactly one child wrapper (JobObject or
        // ProcessGroup); KillOnDrop and CreationFlags only configure the command.
        // Its direct inner layer is the native child. The adoption path is
        // already native and its inner_mut() returns itself. Do not use the
        // dependency's unsafe arbitrary-depth native-child escape hatch.
        let status = self.child.inner_mut().wait().await?;
        self.start_kill()?;
        Ok(status)
    }

    /// Terminates the owned job/group once, without waiting for root exit.
    ///
    /// # Errors
    /// Returns the operating-system error if termination was refused. An already
    /// empty Unix group is successful cleanup, not a reason to signal a saved PID.
    pub fn start_kill(&mut self) -> io::Result<()> {
        if let Some(error) = &self.termination_error {
            return Err(io::Error::other(error.clone()));
        }
        if !std::mem::replace(&mut self.terminate_on_drop, false) {
            return Ok(());
        }
        match self.child.start_kill() {
            #[cfg(unix)]
            Err(error) if error.raw_os_error() == Some(nix::errno::Errno::ESRCH as i32) => Ok(()),
            Err(error) => {
                self.termination_error = Some(error.to_string());
                Err(error)
            }
            Ok(()) => Ok(()),
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let _ = self.start_kill();
    }
}

/// A spawned asynchronous task that is aborted when its owner is dropped.
///
/// In contrast, dropping Tokio's plain `JoinHandle` detaches the task. Reader
/// futures must not keep pipes and buffers alive after their command has ended.
pub struct AbortOnDrop<T> {
    handle: JoinHandle<T>,
}

impl<T> AbortOnDrop<T> {
    /// Takes ownership of a spawned task.
    #[must_use]
    pub const fn new(handle: JoinHandle<T>) -> Self {
        Self { handle }
    }
}

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, JoinError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().handle).poll(context)
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[cfg(test)]
#[path = "owned_process_tests.rs"]
mod tests;
