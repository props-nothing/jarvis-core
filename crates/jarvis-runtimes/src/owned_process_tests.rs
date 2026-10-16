#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::*;

struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

async fn reader_task() -> (AbortOnDrop<()>, Arc<AtomicBool>) {
    let dropped = Arc::new(AtomicBool::new(false));
    let signal = Dropped(Arc::clone(&dropped));
    let (started, running) = tokio::sync::oneshot::channel();
    let task = AbortOnDrop::new(tokio::spawn(async move {
        let _signal = signal;
        let _ = started.send(());
        std::future::pending::<()>().await;
    }));
    running.await.unwrap();
    (task, dropped)
}

async fn assert_task_aborted(dropped: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the reader's resources must be dropped"));
}

#[tokio::test]
async fn dropping_the_owner_aborts_a_reader_instead_of_detaching_it() {
    let (task, dropped) = reader_task().await;
    drop(task);
    assert_task_aborted(&dropped).await;
}

#[tokio::test]
async fn a_reader_deadline_aborts_the_task_instead_of_detaching_it() {
    let (task, dropped) = reader_task().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(10), task)
            .await
            .is_err()
    );
    assert_task_aborted(&dropped).await;
}

#[tokio::test]
async fn refused_tree_cleanup_stays_failed_after_root_exit_and_is_not_retried_by_pid() {
    use std::sync::atomic::AtomicUsize;

    #[derive(Debug)]
    struct RefuseTermination {
        child: Box<dyn ChildWrapper>,
        attempts: Arc<AtomicUsize>,
    }
    impl ChildWrapper for RefuseTermination {
        fn inner(&self) -> &dyn ChildWrapper {
            self.child.as_ref()
        }
        fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
            self.child.as_mut()
        }
        fn into_inner(self: Box<Self>) -> Box<dyn ChildWrapper> {
            self.child
        }
        fn start_kill(&mut self) -> io::Result<()> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "tree termination denied",
            ))
        }
    }

    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit 0"]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 0"]);
        command
    };
    command.kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let child = command.spawn().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut owned = OwnedProcess {
        child: Box::new(RefuseTermination {
            child: Box::new(child),
            attempts: Arc::clone(&attempts),
        }),
        terminate_on_drop: true,
        termination_error: None,
    };
    assert!(
        owned.wait().await.is_err(),
        "root success cannot conceal refused tree cleanup"
    );
    assert!(
        owned.wait().await.is_err(),
        "a second wait cannot erase refused cleanup"
    );
    drop(owned);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "no later signal may target an old group identifier"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn failed_job_assignment_never_resumes_the_suspended_process() {
    use std::sync::atomic::AtomicU32;

    use process_wrap::tokio::CommandWrapper;

    // A terminal wrapper deliberately withholds the native process handle. The
    // real JobObject wrapper must fail setup, kill it and never resume its code.
    #[derive(Debug)]
    struct HideHandle(Arc<AtomicU32>);
    #[derive(Debug)]
    struct HiddenChild(Box<dyn ChildWrapper>);
    impl ChildWrapper for HiddenChild {
        fn inner(&self) -> &dyn ChildWrapper {
            self
        }
        fn inner_mut(&mut self) -> &mut dyn ChildWrapper {
            self
        }
        fn into_inner(self: Box<Self>) -> Box<dyn ChildWrapper> {
            self
        }
        fn start_kill(&mut self) -> io::Result<()> {
            self.0.start_kill()
        }
    }
    impl CommandWrapper for HideHandle {
        fn wrap_child(
            &mut self,
            child: Box<dyn ChildWrapper>,
            _: &CommandWrap,
        ) -> io::Result<Box<dyn ChildWrapper>> {
            self.0.store(child.id().unwrap(), Ordering::SeqCst);
            Ok(Box::new(HiddenChild(child)))
        }
    }

    let marker =
        std::env::temp_dir().join(format!("job-refused-{}.txt", jarvis_core::scratch_tag()));
    let mut command = Command::new("cmd");
    command.args(["/C", &format!("echo must-not-run>\"{}\"", marker.display())]);
    let mut wrapped = CommandWrap::from(command);
    let pid = Arc::new(AtomicU32::new(0));
    wrapped.wrap(KillOnDrop);
    wrapped.wrap(process_wrap::tokio::CreationFlags(
        windows::Win32::System::Threading::CREATE_NO_WINDOW,
    ));
    wrapped.wrap(HideHandle(Arc::clone(&pid)));
    wrapped.wrap(process_wrap::tokio::JobObject);
    let result = OwnedProcess::spawn_wrapped(wrapped);
    assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::Unsupported));
    let pid = pid.load(Ordering::SeqCst);
    assert_ne!(pid, 0, "the refusal must occur after native spawn");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let output = std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .unwrap();
            assert!(output.status.success());
            if !String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("refused suspended child must exit"));
    assert!(
        !marker.exists(),
        "code ran before job ownership was established"
    );
}
