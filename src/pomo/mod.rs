use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    thread,
};

use chrono::{DateTime, Duration, Utc};
use thiserror::Error;
use tokio::sync::oneshot::{channel as oneshot_channel, error::TryRecvError, Receiver, Sender};
use tracing::{debug, field, instrument, trace, Span};
use uuid::Uuid;

/// An active pomocop session.
pub struct Session {
    id: Uuid,
    config: SessionConfig,
    current_phase: Option<PhaseHandle>,
}

impl Session {
    /// Create a session from the given [`SessionConfig`], without starting it.
    fn from_config(config: SessionConfig) -> Self {
        Self {
            id: Uuid::new_v4(),
            config,
            current_phase: None,
        }
    }

    /// Get the ID of this session.
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn advance(&mut self) -> Phase {
        let (send, recv) = oneshot_channel();

        let start = Utc::now();
        let end = start + Duration::minutes(self.config.work as i64);

        self.current_phase = Some(PhaseHandle { start, send });

        Phase {
            session: self.id,
            end,
            recv,
            waker: None,
        }
    }

    pub fn skip(&mut self) -> Result<Phase, SessionError> {
        if let Some(phase) = self.current_phase.take() {
            phase
                .send
                .send(PhaseMessage::Skip)
                .map_err(|_| SessionError::Skip)
                .map(|_| self.advance())
        } else {
            Err(SessionError::NotActive)
        }
    }

    pub fn stop(&mut self) -> Result<(), SessionError> {
        if let Some(phase) = self.current_phase.take() {
            phase
                .send
                .send(PhaseMessage::Stop)
                .map_err(|_| SessionError::Stop)
        } else {
            Err(SessionError::NotActive)
        }
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("failed to skip phase")]
    Skip,

    #[error("failed to stop session")]
    Stop,

    #[error("session is not active")]
    NotActive,
}

enum PhaseMessage {
    Stop,
    Skip,
}

pub struct PhaseHandle {
    start: DateTime<Utc>,
    send: Sender<PhaseMessage>,
}

#[derive(Debug)]
pub enum PhaseResult {
    Completed,
    Skipped,
    Stopped,
    Failed,
}

#[must_use]
pub struct Phase {
    session: Uuid,
    end: DateTime<Utc>,
    recv: Receiver<PhaseMessage>,
    waker: Option<(Arc<Mutex<Waker>>, Receiver<()>)>,
}

impl Future for Phase {
    type Output = PhaseResult;

    #[instrument(skip(self, ctx), fields(session = field::Empty))]
    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        let span = Span::current();
        span.record("session", &format!("{:?}", self.session).as_str());

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
        // we've just found out that the previous one is
        if self.waker.is_none() {
            let when = Utc::now() + Duration::milliseconds(100);

            let (send, recv) = oneshot_channel();
            let waker = Arc::new(Mutex::new(ctx.waker().clone()));
            self.waker = Some((waker.clone(), recv));

            thread::spawn(move || {
                let now = Utc::now();

                if now < when {
                    let duration = (when - now)
                        .to_std()
                        .expect("duration is not negative, we just checked");

                    thread::sleep(duration);
                }

                send.send(())
                    .expect("receiver should not have been dropped yet");

                let waker = waker.lock().unwrap();
                waker.wake_by_ref();
            });
        }

        match self.recv.try_recv() {
            Ok(PhaseMessage::Skip) => {
                debug!("phase skipped");
                Poll::Ready(PhaseResult::Skipped)
            }
            Ok(PhaseMessage::Stop) => {
                debug!("phase stopped");
                Poll::Ready(PhaseResult::Stopped)
            }
            Err(TryRecvError::Closed) => {
                debug!("phase failed");
                Poll::Ready(PhaseResult::Failed)
            }
            Err(TryRecvError::Empty) => {
                let now = Utc::now();
                let is_finished = now >= self.end;

                if is_finished {
                    debug!("phase completed");
                    Poll::Ready(PhaseResult::Completed)
                } else {
                    trace!("phase still pending");
                    Poll::Pending
                }
            }
        }
    }
}

/// A pomocop session configuration, defining the lengths of each of the three
/// types of phase, and the interval between long breaks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionConfig {
    work: usize,
    short: usize,
    long: usize,
    interval: usize,
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
