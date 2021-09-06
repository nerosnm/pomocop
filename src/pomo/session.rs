use std::{
    collections::HashSet,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    thread,
};

use chrono::{DateTime, Duration, Utc};
use poise::serenity_prelude as serenity;
use serenity::UserId;
use tap::TapFallible;
use thiserror::Error;
use tokio::sync::oneshot::{channel as oneshot_channel, error::TryRecvError, Receiver, Sender};
use tracing::{debug, instrument, trace, warn};
use uuid::Uuid;

/// An active pomocop session.
#[derive(Debug)]
pub struct Session {
    id: Uuid,
    members: HashSet<UserId>,
    config: SessionConfig,
    current_phase: Option<PhaseHandle>,
    next_index: usize,
}

impl Session {
    /// Create a session from the given [`SessionConfig`], without starting it.
    fn from_config(config: SessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            members: HashSet::new(),
            config,
            current_phase: None,
            next_index: 0,
        }
    }

    /// Get the ID of this session.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get the config of this session.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// Add a user to the set of members of this session.
    ///
    /// Returns whether the user was added (i.e. `true` if the user was not
    /// already a member, `false` otherwise).
    pub fn add_member(&mut self, user: UserId) -> bool {
        self.members.insert(user)
    }

    /// Remove a user from the set of members of this session.
    ///
    /// Returns whether the user was a member.
    pub fn remove_member(&mut self, user: UserId) -> bool {
        self.members.remove(&user)
    }

    /// Get the set of members of this session
    pub fn members(&self) -> &HashSet<UserId> {
        &self.members
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
        let end = start + Duration::minutes(phase_type.length() as i64);

        self.current_phase = Some(PhaseHandle {
            started: start,
            phase_type,
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
    /// means that the phase finished on its own). If there was a currently
    /// running phase, returns its type.
    #[instrument]
    pub fn skip(&mut self) -> Result<PhaseType, SessionError> {
        if let Some(phase) = self.current_phase.take() {
            phase
                .send
                .send(PhaseMessage::Skip)
                .tap_err(|_| warn!("unable to skip phase; did it complete on its own?"))
                .ok();

            Ok(phase.phase_type)
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

    pub fn status(&self) -> SessionStatus {
        match self.current_phase {
            Some(ref phase) => SessionStatus::Running {
                phase_type: phase.phase_type,
                phase_elapsed: phase.elapsed(),
                phase_remaining: phase.remaining(),
                next_type: self.config.phase_at(self.next_index),
                long_at: Utc::now()
                    + phase.remaining()
                    + Duration::minutes(self.config.until_long(self.next_index) as i64),
            },
            None => SessionStatus::NoSession,
        }
    }
}

#[derive(Debug)]
pub enum SessionStatus {
    NoSession,
    Running {
        phase_type: PhaseType,
        phase_elapsed: Duration,
        phase_remaining: Duration,
        next_type: PhaseType,
        long_at: DateTime<Utc>,
    },
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
    phase_type: PhaseType,
    send: Sender<PhaseMessage>,
}

impl PhaseHandle {
    fn elapsed(&self) -> Duration {
        Utc::now() - self.started
    }

    fn remaining(&self) -> Duration {
        Duration::minutes(self.phase_type.length() as i64) - self.elapsed()
    }
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

    /// Return the number of minutes between the beginning of the phase with
    /// index `current` and the beginning of the next long break.
    fn until_long(&self, mut current: usize) -> usize {
        let mut minutes = 0;

        while let PhaseType::Work(length) | PhaseType::Short(length) = self.phase_at(current) {
            minutes += length;
            current += 1;
        }

        minutes
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
