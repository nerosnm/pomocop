use indoc::formatdoc;
use tracing::{error, info, instrument};

use crate::{
    pomo::session::{PhaseResult, Session, SessionConfig, SessionError},
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
        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                Unable to start pomocop session.

                There is already a running session in this channel!
                "
            })
        })
        .await?;

        Ok(())
    } else {
        let config = SessionConfig::default()
            .work_or_default(work)
            .short_or_default(short)
            .long_or_default(long)
            .interval_or_default(interval);

        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                Starting pomocop session.

                Working for {work} minutes at a time, with a {short} minute short break after each work session, and a {long} minute long break after every {interval}.
                ",
                work = config.work,
                short = config.short,
                long = config.long,
                interval = config.interval,
            })
        })
        .await?;

        let session = config.build();
        info!(?session, "created new session");

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
        let phase = sessions
            .get_mut(&ctx.channel_id())
            .expect("session stays in sessions until we remove it")
            .advance();
        drop(sessions);

        info!(phase_type = ?phase.phase_type(), "starting next phase");

        if let Err(error) = ctx
            .channel_id()
            .send_message(&ctx.discord().http, |msg| {
                msg.content(formatdoc! { "
                    Finished a {finished}, starting a {next}!
                    ",
                    finished = finished.description(),
                    next = phase.phase_type().description(),
                })
                .tts(true)
            })
            .await
        {
            error!(?error, "unable to send phase change message");
        }

        result = phase.await;
    }

    match result {
        PhaseResult::Stopped(_) => {
            info!(?result, "session stopped");

            if let Err(error) = ctx
                .channel_id()
                .send_message(&ctx.discord().http, |msg| {
                    msg.content(formatdoc! { "
                        Session stopped!
                        ",
                    })
                    .tts(true)
                })
                .await
            {
                error!(?error, "unable to send session stopped message");
            }
        }
        PhaseResult::Failed(_) => {
            error!(?result, "session failed");

            if let Err(error) = ctx
                .channel_id()
                .send_message(&ctx.discord().http, |msg| {
                    msg.content(formatdoc! { "
                        Session {id} failed!
                        ",
                        id = id,
                    })
                    .tts(true)
                })
                .await
            {
                error!(?error, "unable to send session failed message");
            }
        }
        PhaseResult::Completed(_) | PhaseResult::Skipped(_) => unreachable!(),
    }

    let mut sessions = ctx.data().sessions.lock().await;
    sessions.remove(&ctx.channel_id());

    Ok(())
}

/// Skip the current phase of the pomo session running in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        match session.skip() {
            Ok(()) => {
                poise::send_reply(ctx, |reply| {
                    reply.content(formatdoc! { "
                        Skipping phase...
                        "
                    })
                })
                .await?;
            }
            Err(SessionError::NotActive) => {
                poise::send_reply(ctx, |reply| {
                    reply.content(formatdoc! { "
                        Unable to skip current phase.

                        It may have completed on its own. Please check if the phase already advanced, and if not, try again.
                        "
                    })
                })
                .await?;
            }
        }
    } else {
        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                Unable to skip current phase.

                There is no running session in this channel!
                "
            })
        })
        .await?;
    }

    Ok(())
}

/// Stop the pomo session currently running in this channel
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    if let Some(session) = ctx.data().sessions.lock().await.get_mut(&ctx.channel_id()) {
        match session.stop() {
            Ok(()) => {
                poise::send_reply(ctx, |reply| {
                    reply.content(formatdoc! { "
                        Stopping session...
                        "
                    })
                })
                .await?;
            }
            Err(SessionError::NotActive) => {
                poise::send_reply(ctx, |reply| {
                    reply.content(formatdoc! { "
                        Unable to stop session.

                        Please try again.
                        "
                    })
                })
                .await?;
            }
        }
    } else {
        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                Unable to stop session.

                There is no running session in this channel!
                "
            })
        })
        .await?;
    }

    Ok(())
}
