//! Process-local coordination between work admission and graceful shutdown.

use std::sync::{Arc, Mutex};

/// A shared admission gate for one daemon incarnation, not a durable task ledger.
#[derive(Clone, Debug, Default)]
pub struct WorkAdmission(Arc<Mutex<State>>);

#[derive(Debug, Default)]
struct State {
    phase: Phase,
    active: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Open,
    Checking,
    Closed,
}

/// A refusal to admit work or reserve shutdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkAdmissionError {
    /// Another operation owns a permit; ordinary shutdown must leave admission open.
    #[error("work is still active ({active} operations)")]
    Busy {
        /// Active permits, which may overlap durable run and task counts.
        active: usize,
    },
    /// Shutdown has reserved or permanently closed admission.
    #[error("the daemon is stopping; retry after it restarts")]
    Stopping,
    /// The gate cannot be trusted.
    #[error("work admission state is unavailable")]
    Unavailable,
}

/// Keeps an admitted operation visible until its effects are published or it finishes.
#[derive(Debug)]
#[must_use]
pub struct WorkPermit(WorkAdmission);

/// Freezes admission while shutdown checks durable and process state.
/// Dropping an uncommitted reservation reopens admission, including on cancellation.
#[derive(Debug)]
#[must_use]
pub struct ShutdownPermit(WorkAdmission);

impl WorkAdmission {
    /// Reserves one operation until the returned permit is dropped.
    ///
    /// # Errors
    /// Returns an error when shutdown has started or the gate is unavailable.
    pub fn admit(&self) -> Result<WorkPermit, WorkAdmissionError> {
        let mut state = self.0.lock().map_err(|_| WorkAdmissionError::Unavailable)?;
        if state.phase != Phase::Open {
            return Err(WorkAdmissionError::Stopping);
        }
        state.active = state
            .active
            .checked_add(1)
            .ok_or(WorkAdmissionError::Unavailable)?;
        Ok(WorkPermit(self.clone()))
    }

    /// Freezes new admissions before checking the durable work snapshot.
    /// `None` means shutdown was already committed (a repeated Stop may acknowledge it).
    /// Force bypasses active work, but still closes admission to new operations.
    ///
    /// # Errors
    /// Refuses concurrent shutdown checks, active work without force, or an unavailable gate.
    pub fn prepare_shutdown(
        &self,
        force: bool,
    ) -> Result<Option<ShutdownPermit>, WorkAdmissionError> {
        let mut state = self.0.lock().map_err(|_| WorkAdmissionError::Unavailable)?;
        match state.phase {
            Phase::Closed => return Ok(None),
            Phase::Checking => return Err(WorkAdmissionError::Stopping),
            Phase::Open => {}
        }
        if !force && state.active != 0 {
            return Err(WorkAdmissionError::Busy {
                active: state.active,
            });
        }
        state.phase = Phase::Checking;
        Ok(Some(ShutdownPermit(self.clone())))
    }

    /// Permanently closes this incarnation, including on an OS signal or poisoned lock.
    pub fn close(&self) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.phase = Phase::Closed;
    }
}

impl ShutdownPermit {
    /// Makes a successful shutdown reservation permanent.
    pub fn commit(self) {
        self.0.close();
    }

    /// Runs a synchronous launch and publishes its success under the gate lock.
    /// A concurrently starting restart helper cannot observe a half-finished launch.
    /// The callback must not use this gate; `false` reopens admission on return.
    ///
    /// # Errors
    /// Returns an error without launching if shutdown was superseded or the lock is poisoned.
    pub fn commit_if(self, launch: impl FnOnce() -> bool) -> Result<(), WorkAdmissionError> {
        let mut state = self
            .0
            .0
            .lock()
            .map_err(|_| WorkAdmissionError::Unavailable)?;
        if state.phase != Phase::Checking {
            return Err(WorkAdmissionError::Stopping);
        }
        if launch() {
            state.phase = Phase::Closed;
        }
        Ok(())
    }
}

impl Drop for ShutdownPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.lock()
            && state.phase == Phase::Checking
        {
            state.phase = Phase::Open;
        }
    }
}

impl Drop for WorkPermit {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.lock() {
            state.active -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn active_work_refuses_without_disrupting_nested_admissions() {
        let gate = WorkAdmission::default();
        let first = gate.admit().unwrap();
        assert!(matches!(
            gate.prepare_shutdown(false),
            Err(WorkAdmissionError::Busy { active: 1 })
        ));
        drop(gate.admit().unwrap());
        drop(first);
        gate.prepare_shutdown(false).unwrap().unwrap().commit();
        assert!(matches!(gate.admit(), Err(WorkAdmissionError::Stopping)));
    }

    #[test]
    fn abandoned_checks_reopen_but_committed_or_signalled_shutdown_does_not() {
        let gate = WorkAdmission::default();
        let reservation = gate.prepare_shutdown(false).unwrap().unwrap();
        assert!(matches!(gate.admit(), Err(WorkAdmissionError::Stopping)));
        assert!(matches!(
            gate.prepare_shutdown(true),
            Err(WorkAdmissionError::Stopping)
        ));
        drop(reservation);
        drop(gate.admit().unwrap());
        let reservation = gate.prepare_shutdown(false).unwrap().unwrap();
        gate.close();
        drop(reservation);
        assert!(matches!(gate.admit(), Err(WorkAdmissionError::Stopping)));
        assert!(gate.prepare_shutdown(false).unwrap().is_none());
    }

    #[test]
    fn force_closes_admission_even_with_active_work() {
        let gate = WorkAdmission::default();
        let work = gate.admit().unwrap();
        gate.prepare_shutdown(true).unwrap().unwrap().commit();
        drop(work);
        assert!(matches!(gate.admit(), Err(WorkAdmissionError::Stopping)));
    }

    #[test]
    fn admission_and_shutdown_race_has_exactly_one_winner() {
        for _ in 0..100 {
            let gate = WorkAdmission::default();
            let start = Arc::new(std::sync::Barrier::new(2));
            let other_gate = gate.clone();
            let other_start = Arc::clone(&start);
            let worker = std::thread::spawn(move || {
                other_start.wait();
                other_gate.admit()
            });
            start.wait();
            let shutdown = gate.prepare_shutdown(false);
            let work = worker.join().unwrap();
            assert_ne!(shutdown.is_ok(), work.is_ok());
        }
    }

    #[test]
    fn poisoned_gate_fails_closed() {
        let gate = WorkAdmission::default();
        let other = gate.clone();
        let _ = std::thread::spawn(move || {
            let _lock = other.0.lock().unwrap();
            panic!("poison gate");
        })
        .join();
        assert!(matches!(gate.admit(), Err(WorkAdmissionError::Unavailable)));
        assert!(matches!(
            gate.prepare_shutdown(true),
            Err(WorkAdmissionError::Unavailable)
        ));
        gate.close();
        assert!(gate.admit().is_err());
    }

    #[test]
    fn restart_launch_is_published_before_the_helper_can_check_shutdown() {
        let gate = WorkAdmission::default();
        let reservation = gate.prepare_shutdown(false).unwrap().unwrap();
        let start = Arc::new(std::sync::Barrier::new(2));
        let other_start = Arc::clone(&start);
        let other_gate = gate.clone();
        let helper = std::thread::spawn(move || {
            other_start.wait();
            other_gate.prepare_shutdown(false)
        });
        reservation
            .commit_if(|| {
                start.wait();
                true
            })
            .unwrap();
        assert!(helper.join().unwrap().unwrap().is_none());

        let gate = WorkAdmission::default();
        gate.prepare_shutdown(false)
            .unwrap()
            .unwrap()
            .commit_if(|| false)
            .unwrap();
        drop(gate.admit().unwrap());
    }
}
