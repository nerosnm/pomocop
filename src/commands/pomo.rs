use indoc::formatdoc;
use tracing::{error, info, instrument};

use crate::{
    pomo::{PhaseResult, Session, SessionConfig},
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

#[instrument(skip(ctx, session), fields(id = %session.id()))]
async fn run_session(ctx: Context<'_>, mut session: Session) -> Result<(), Error> {
    let phase = session.advance();
    info!(phase_type = ?phase.phase_type(), "starting first phase");

    let mut result = phase.await;

    while let PhaseResult::Completed(finished) | PhaseResult::Skipped(finished) = result {
        info!(?result, "finished phase");

        let phase = session.advance();
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
                        id = session.id(),
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

    Ok(())
}
