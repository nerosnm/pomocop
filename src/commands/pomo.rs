use chrono_tz::{Tz, UTC};
use tracing::{error, info, instrument};

use crate::{
    pomo::{
        reply::{
            reply_cannot_start, reply_join_already_member, reply_join_no_session, reply_joined,
            reply_leave_no_session, reply_leave_not_member, reply_left, reply_skip_failed,
            reply_skip_no_session, reply_skipping_phase, reply_starting, reply_status,
            reply_status_no_session, reply_stop_failed, reply_stop_no_session,
            reply_stopping_session, say_phase_finished, say_session_failed, say_session_stopped,
        },
        session::{PhaseResult, Session, SessionConfig, SessionError, SessionStatus},
    },
    Context, Error,
};

/// Start a pomo session in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn start(
    ctx: Context<'_>,
    #[description = "Length of a work session in minutes (default: 25)"] work: Option<usize>,
    #[description = "Length of a short break in minutes (default: 5)"] short: Option<usize>,
    #[description = "Length of a long break in minutes (default: 15)"] long: Option<usize>,
    #[description = "How many work sessions between each long break (default: 4)"] interval: Option<
        usize,
    >,
) -> Result<(), Error> {
    if ctx
        .data()
        .sessions
        .lock()
        .await
        .contains_key(&ctx.channel_id())
    {
        reply_cannot_start(ctx).await;

        Ok(())
    } else {
        let config = SessionConfig::default()
            .work_or_default(work)
            .short_or_default(short)
            .long_or_default(long)
            .interval_or_default(interval);

        let mut session = config.build();
        session.add_member(ctx.author().id);

        info!(?session, "created new session");

        reply_starting(ctx, session.config(), session.id()).await;

        run_session(ctx, session).await
    }
}

#[instrument(skip(ctx, session), fields(id = %session.id()))]
async fn run_session(ctx: Context<'_>, session: Session) -> Result<(), Error> {
    let id = session.id();

    let mut sessions = ctx.data().sessions.lock().await;
    sessions.insert(ctx.channel_id(), session);

    let phase = sessions
        .get_mut(&ctx.channel_id())
        .expect("session stays in sessions until we remove it")
        .advance();

    drop(sessions);

    info!(phase_type = ?phase.phase_type(), "starting first phase");
    let mut result = phase.await;

    while let PhaseResult::Completed(finished) | PhaseResult::Skipped(finished) = result {
        info!(?result, "finished phase");

        let mut sessions = ctx.data().sessions.lock().await;
        let session = sessions
            .get_mut(&ctx.channel_id())
            .expect("session stays in sessions until we remove it");

        let phase = session.advance();
        let members = session.members().iter();

        info!(phase_type = ?phase.phase_type(), "starting next phase");

        say_phase_finished(ctx, finished, *phase.phase_type(), members).await;

        drop(sessions);

        result = phase.await;
    }

    match result {
        PhaseResult::Stopped(_) => {
            info!(?result, "session stopped");

            say_session_stopped(ctx).await;
        }
        PhaseResult::Failed(_) => {
            error!(?result, "session failed");

            say_session_failed(ctx, id).await;
        }
        PhaseResult::Completed(_) | PhaseResult::Skipped(_) => unreachable!(),
    }

    let mut sessions = ctx.data().sessions.lock().await;
    sessions.remove(&ctx.channel_id());

    Ok(())
}

/// Get the status of the current pomo session running in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn status(
    ctx: Context<'_>,
    #[description = "Your time zone (example: Europe/London, default: UTC)"] timezone: Option<
        String,
    >,
) -> Result<(), Error> {
    let tz: Tz = timezone
        .and_then(|tz_str| tz_str.parse().ok())
        .unwrap_or(UTC);

    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        match session.status() {
            SessionStatus::Running {
                phase_type,
                phase_elapsed,
                phase_remaining,
                next_type,
                long_at,
            } => {
                reply_status(
                    ctx,
                    phase_type,
                    phase_elapsed,
                    phase_remaining,
                    next_type,
                    long_at,
                    tz,
                )
                .await
            }
            SessionStatus::NoSession => reply_status_no_session(ctx).await,
        }
    } else {
        reply_status_no_session(ctx).await;
    }

    Ok(())
}

/// Join the pomo session running in this channel to be notified when phases
/// finish
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn join(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        if session.add_member(ctx.author().id) {
            reply_joined(ctx).await;
        } else {
            reply_join_already_member(ctx).await;
        }
    } else {
        reply_join_no_session(ctx).await;
    }

    Ok(())
}

/// Leave the pomo session running in this channel to stop being notified
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        if session.remove_member(ctx.author().id) {
            reply_left(ctx).await;
        } else {
            reply_leave_not_member(ctx).await;
        }
    } else {
        reply_leave_no_session(ctx).await;
    }

    Ok(())
}

/// Skip the current phase of the pomo session running in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        match session.skip() {
            Ok(skipped_type) => reply_skipping_phase(ctx, skipped_type).await,
            Err(SessionError::NotActive) => reply_skip_failed(ctx, session.id()).await,
        }
    } else {
        reply_skip_no_session(ctx).await;
    }

    Ok(())
}

/// Stop the pomo session currently running in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        match session.stop() {
            Ok(()) => reply_stopping_session(ctx).await,
            Err(SessionError::NotActive) => reply_stop_failed(ctx, session.id()).await,
        }
    } else {
        reply_stop_no_session(ctx).await;
    }

    Ok(())
}
