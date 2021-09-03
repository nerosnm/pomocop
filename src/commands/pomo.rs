use indoc::formatdoc;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, instrument, warn};

use crate::{Context, Error};

/// Start a pomo session in this server
#[instrument(skip(ctx))]
#[poise::command(track_edits, slash_command)]
pub async fn start(
    ctx: Context<'_>,
    #[description = "Duration of a work session in minutes (default: 25)"] work: Option<usize>,
    #[description = "Duration of a short break in minutes (default: 5)"] short: Option<usize>,
    #[description = "Duration of a long break in minutes (default: 15)"] long: Option<usize>,
    #[description = "Number of work sessions to run (default: 8)"] sessions: Option<usize>,
    #[description = "How many work sessions between each long break (default: 4)"] interval: Option<
        usize,
    >,
) -> Result<(), Error> {
    let work = work.unwrap_or(25);
    let short = short.unwrap_or(5);
    let long = long.unwrap_or(15);
    let sessions = sessions.unwrap_or(8);
    let interval = interval.unwrap_or(4);

    if check_running(&ctx, true)? {
        warn!("session already running, refusing to start a new one");

        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                There is already an active session in this channel! Please run the `stop` command to stop it.
                ",
            })
        })
        .await?;

        return Ok(());
    }

    info!("starting a pomo session");

    poise::send_reply(ctx, |reply| {
        reply.content(formatdoc! { "
            Starting pomocop session.

            Working for {sessions} sessions of {work} minutes each, with a {short} minute short break after each one, and a {long} minute long break after every {interval} work sessions.
            ",
            sessions = sessions,
            work = work,
            short = short,
            long = long,
            interval = interval,
        })
    })
    .await?;

    for session in 0..sessions {
        debug!(?session, "starting work session");
        sleep(Duration::from_secs(work as u64 * 60)).await;

        if !check_running(&ctx, false)? {
            break;
        }

        if session % interval == (interval - 1) {
            debug!(
                ?session,
                ?interval,
                modulo = session % interval,
                "starting long break",
            );

            ctx.channel_id()
                .send_message(&ctx.discord().http, |msg| {
                    msg.content(formatdoc! { "
                        Finished work session #{session_num}! Starting a {long} minute long break!
                        ",
                        session_num = session + 1,
                        long = long,
                    })
                    .tts(true)
                })
                .await?;

            sleep(Duration::from_secs(long as u64 * 60)).await;
        } else {
            debug!(
                ?session,
                ?interval,
                modulo = session % interval,
                "starting short break",
            );

            ctx.channel_id()
                .send_message(&ctx.discord().http, |msg| {
                    msg.content(formatdoc! { "
                        Finished work session #{session_num}! Starting a {short} minute short break!
                        ",
                        session_num = session + 1,
                        short = short,
                    })
                    .tts(true)
                })
                .await?;

            sleep(Duration::from_secs(short as u64 * 60)).await;
        }

        if check_running(&ctx, false)? {
            if session == sessions - 1 {
                ctx.channel_id()
                    .send_message(&ctx.discord().http, |msg| {
                        msg.content(formatdoc! { "
                            You made it through {sessions} work sessions! Nice job!
                            ",
                            sessions = sessions,
                        })
                        .tts(true)
                    })
                    .await?;
            } else {
                ctx.channel_id()
                    .send_message(&ctx.discord().http, |msg| {
                        msg.content(formatdoc! { "
                            Break over! Back to work! Wee woo wee woo!
                            ",
                        })
                        .tts(true)
                    })
                    .await?;
            }
        } else {
            break;
        }
    }

    info!("finished pomo session");

    {
        let mut sessions = ctx.data().sessions.lock().unwrap();
        sessions.insert(ctx.channel_id(), false);
    }

    Ok(())
}

/// Stop the running pomo session in this server, if there is one
#[instrument(skip(ctx))]
#[poise::command(track_edits, slash_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    if !check_running(&ctx, false)? {
        warn!("no session running, refusing to stop one");

        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                There is no active session in this channel! Please run the `start` command to start one.
                ",
            })
        })
        .await?;
    } else {
        info!("stopping pomo session");

        {
            let mut sessions = ctx.data().sessions.lock().unwrap();
            sessions.insert(ctx.channel_id(), false);
        }

        poise::send_reply(ctx, |reply| {
            reply.content(formatdoc! { "
                Stopped pomocop session.
                ",
            })
        })
        .await?;
    }

    Ok(())
}

/// Check the contents of `ctx.data` to see if there is a running session in
/// `ctx.channel_id()`.
///
/// If no session is running and `set_running` is `true`, then the value will be
/// changed to `true`.
fn check_running(ctx: &Context<'_>, set_running: bool) -> Result<bool, Error> {
    let mut sessions = ctx.data().sessions.lock().unwrap();

    let running = sessions.entry(ctx.channel_id()).or_insert(false);
    let was_running = *running;

    if !was_running && set_running {
        *running = true;
    };

    Ok(was_running)
}
