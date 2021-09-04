use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    thread,
};

use chrono::{DateTime, Duration, Utc};
use tap::TapFallible;
use thiserror::Error;
use tokio::sync::oneshot::{channel as oneshot_channel, error::TryRecvError, Receiver, Sender};
use tracing::{debug, instrument, trace, warn};
use uuid::Uuid;

/// An active pomocop session.
#[derive(Debug)]
pub struct Session {
    id: Uuid,
    config: SessionConfig,
    current_phase: Option<PhaseHandle>,
    next_index: usize,
}

impl Session {
    /// Create a session from the given [`SessionConfig`], without starting it.
    fn from_config(config: SessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            config,
            current_phase: None,
            next_index: 0,
        }
    }

    /// Get the ID of this session.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Unconditionally advance to the next phase and return it, regardless of
    /// whether there is a running phase already.
    ///
    /// In the process, this will drop the stored [`PhaseHandle`], making it
    /// impossible to skip or stop a running phase. If there is a possibility
    /// that a phase is still running, [`Session::skip()`] or
    /// [`Session::stop()`] should be used instead.
    #[instrument]
    pub fn advance(&mut self) -> Phase {
        let (send, recv) = oneshot_channel();

        let phase_type = self.config.phase_at(self.next_index);
        self.next_index += 1;

        let start = Utc::now();
        let end = start + Duration::seconds(phase_type.length() as i64);

        self.current_phase = Some(PhaseHandle {
            started: start,
            send,
        });

        Phase {
            session: self.id,
            end,
            phase_type,
            recv,
            waker: None,
        }
    }

    /// Skip the currently running phase.
    ///
    /// Returns [`SessionError::NotActive`] if there is no currently running
    /// phase, or if it was not possible to send the skip message (which likely
    /// means that the phase finished on its own).
    #[instrument]
    pub fn skip(&mut self) -> Result<(), SessionError> {
        if let Some(phase) = self.current_phase.take() {
            phase
                .send
                .send(PhaseMessage::Skip)
                .tap_err(|_| warn!("unable to skip phase; did it complete on its own?"))
                .or(Ok(()))
        } else {
            Err(SessionError::NotActive)
        }
    }

    /// Stop the session by stopping the currently running phase.
    ///
    /// Returns [`SessionError::NotActive`] if there is no currently running
    /// phase, or if it was not possible to send the stop message (which likely
    /// means that the phase finished on its own).
    #[instrument]
    pub fn stop(&mut self) -> Result<(), SessionError> {
        if let Some(phase) = self.current_phase.take() {
            phase
                .send
                .send(PhaseMessage::Stop)
                .tap_err(|_| warn!("unable to stop phase; did it complete on its own?"))
                .map_err(|_| SessionError::NotActive)
        } else {
            Err(SessionError::NotActive)
        }
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("there is no currently active phase")]
    NotActive,
}

/// Messages that can be sent to running [`Phase`]s to instruct them to do
/// things.
enum PhaseMessage {
    /// Stop the phase and resolve to a [`PhaseResult::Skipped`].
    Skip,
    /// Stop the phase and resolve to a [`PhaseResult::Stopped`].
    Stop,
}

/// A handle allowing communication with, and holding details about, a running
/// [`Phase`].
pub struct PhaseHandle {
    started: DateTime<Utc>,
    send: Sender<PhaseMessage>,
}

impl fmt::Debug for PhaseHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Phase")
            .field("started", &self.started)
            .field("send", &"Sender<PhaseMessage>")
            .finish()
    }
}

#[derive(Debug)]
pub enum PhaseResult {
    Completed(PhaseType),
    Skipped(PhaseType),
    Stopped(PhaseType),
    Failed(PhaseType),
}

#[must_use]
pub struct Phase {
    session: Uuid,
    end: DateTime<Utc>,
    phase_type: PhaseType,
    recv: Receiver<PhaseMessage>,
    waker: Option<(Arc<Mutex<Waker>>, Receiver<()>)>,
}

impl Phase {
    pub fn phase_type(&self) -> &PhaseType {
        &self.phase_type
    }
}

impl Future for Phase {
    type Output = PhaseResult;

    #[instrument(skip(self, ctx))]
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        // For more info on this waker logic: https://tokio.rs/tokio/tutorial/async

        if let Some((waker, waker_recv)) = self.waker.as_mut() {
            // First check if the waker thread has signalled that it's finished.
            match waker_recv.try_recv() {
                Ok(()) | Err(TryRecvError::Closed) => {
                    // It has signalled that it's finished, or something has gone wrong and it's
                    // dropped its sender, so in either case we need to create a new one.
                    self.waker = None;
                }
                Err(TryRecvError::Empty) => {
                    // It hasn't sent anything yet, so proceed normally.
                    let mut waker = waker.lock().unwrap();
                    if !waker.will_wake(ctx.waker()) {
                        *waker = ctx.waker().clone();
                    }
                }
            }
        }

        // This will be None either if we haven't spawned a waker thread yet, or if
        // we've just found out that the previous one is finished.
        if self.waker.is_none() {
            let when = Utc::now() + Duration::milliseconds(100);

            let (send, recv) = oneshot_channel();
            let waker = Arc::new(Mutex::new(ctx.waker().clone()));
            self.waker = Some((waker.clone(), recv));

            let session = self.session;

            thread::spawn(move || {
                let span = tracing::debug_span!("waker", id = ?session);
                let _enter = span.enter();

                let now = Utc::now();

                if now < when {
                    let duration = (when - now)
                        .to_std()
                        .expect("duration is not negative, we just checked");

                    thread::sleep(duration);
                }

                match send.send(()) {
                    Ok(()) => {
                        trace!("signalled phase that waker thread has completed");
                    }
                    Err(()) => {
                        debug!(
                            "unable to signal phase that waker thread has completed; phase was \
                             probably dropped"
                        );
                    }
                }

                let waker = waker.lock().unwrap();
                waker.wake_by_ref();
            });
        }

        match self.recv.try_recv() {
            Ok(PhaseMessage::Skip) => {
                debug!("phase skipped");
                Poll::Ready(PhaseResult::Skipped(self.phase_type))
            }
            Ok(PhaseMessage::Stop) => {
                debug!("phase stopped");
                Poll::Ready(PhaseResult::Stopped(self.phase_type))
            }
            Err(TryRecvError::Closed) => {
                debug!("phase failed");
                Poll::Ready(PhaseResult::Failed(self.phase_type))
            }
            Err(TryRecvError::Empty) => {
                let now = Utc::now();
                let is_finished = now >= self.end;

                if is_finished {
                    debug!("phase completed");
                    Poll::Ready(PhaseResult::Completed(self.phase_type))
                } else {
                    trace!("phase still pending");
                    Poll::Pending
                }
            }
        }
    }
}

/// A pomocop session configuration, defining the lengths (in minutes) of each
/// of the three types of phase, and the interval between long breaks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionConfig {
    /// The number of minutes each work phase should last for.
    pub work: usize,
    /// The number of minutes each short break should last for.
    pub short: usize,
    /// The number of minutes each long break should last for.
    pub long: usize,
    /// The number of work sessions in between each long break.
    pub interval: usize,
}

impl SessionConfig {
    pub fn build(self) -> Session {
        Session::from_config(self)
    }

    pub fn work(mut self, work: usize) -> Self {
        self.work = work;
        self
    }

    pub fn work_or_default(self, work: Option<usize>) -> Self {
        if let Some(work) = work {
            self.work(work)
        } else {
            self
        }
    }

    pub fn short(mut self, short: usize) -> Self {
        self.short = short;
        self
    }

    pub fn short_or_default(self, short: Option<usize>) -> Self {
        if let Some(short) = short {
            self.short(short)
        } else {
            self
        }
    }

    pub fn long(mut self, long: usize) -> Self {
        self.long = long;
        self
    }

    pub fn long_or_default(self, long: Option<usize>) -> Self {
        if let Some(long) = long {
            self.long(long)
        } else {
            self
        }
    }

    pub fn interval(mut self, interval: usize) -> Self {
        self.interval = interval;
        self
    }

    pub fn interval_or_default(self, interval: Option<usize>) -> Self {
        if let Some(interval) = interval {
            self.interval(interval)
        } else {
            self
        }
    }

    /// Return the phase type and length for the phase at index `phase_index`.
    fn phase_at(&self, phase_index: usize) -> PhaseType {
        if phase_index % 2 == 0 {
            // The phase index is even, so it's a work phase
            PhaseType::Work(self.work)
        } else if phase_index % (self.interval * 2) == (self.interval * 2 - 1) {
            // The interval refers to how many *work* sessions pass between each long break,
            // so we need to multiply it by 2 to get how many *actual* sessions
            // pass between each long break.
            PhaseType::Long(self.long)
        } else {
            PhaseType::Short(self.short)
        }
    }
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            work: 25,
            short: 5,
            long: 15,
            interval: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseType {
    Work(usize),
    Short(usize),
    Long(usize),
}

impl PhaseType {
    pub fn length(&self) -> usize {
        use PhaseType::*;
        match *self {
            Work(length) | Short(length) | Long(length) => length,
        }
    }

    pub fn description(&self) -> String {
        match *self {
            PhaseType::Work(length) => format!("{} minute work session", length),
            PhaseType::Short(length) => format!("{} minute short break", length),
            PhaseType::Long(length) => format!("{} minute long break", length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_calc() {
        let config = SessionConfig::default();

        let actual = (0..8)
            .into_iter()
            .map(|i| config.phase_at(i))
            .collect::<Vec<_>>();

        let expected = vec![
            PhaseType::Work(config.work),
            PhaseType::Short(config.short),
            PhaseType::Work(config.work),
            PhaseType::Short(config.short),
            PhaseType::Work(config.work),
            PhaseType::Short(config.short),
            PhaseType::Work(config.work),
            PhaseType::Long(config.long),
        ];

        assert_eq!(
            actual, expected,
            "lengths of each session were not calculated correctly"
        );
    }
}
