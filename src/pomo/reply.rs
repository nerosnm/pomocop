use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use hhmmss::Hhmmss;
use indoc::formatdoc;
use poise::{serenity_prelude as serenity, CreateReply};
use serenity::{Color, CreateEmbed, CreateMessage, MessageBuilder};
use tracing::{error, instrument};
use uuid::Uuid;

use crate::{
    pomo::session::{PhaseType, SessionConfig},
    Context,
};

const GREEN: Color = Color::from_rgb(29, 131, 41);
const RED: Color = Color::from_rgb(205, 46, 2);

fn no_footer<B>(builder: B) -> impl FnOnce(&mut CreateEmbed) -> &mut CreateEmbed
where
    B: FnOnce(&mut CreateEmbed) -> &mut CreateEmbed,
{
    |embed| builder(embed).footer(|footer| footer)
}

fn green_embed<B>(
    avatar_url: Option<String>,
    builder: B,
) -> impl FnOnce(&mut CreateEmbed) -> &mut CreateEmbed
where
    B: FnOnce(&mut CreateEmbed) -> &mut CreateEmbed,
{
    embed_with_defaults(avatar_url, GREEN, builder)
}

fn red_embed<B>(
    avatar_url: Option<String>,
    builder: B,
) -> impl FnOnce(&mut CreateEmbed) -> &mut CreateEmbed
where
    B: FnOnce(&mut CreateEmbed) -> &mut CreateEmbed,
{
    embed_with_defaults(avatar_url, RED, builder)
}

fn embed_with_defaults<B>(
    avatar_url: Option<String>,
    color: Color,
    builder: B,
) -> impl FnOnce(&mut CreateEmbed) -> &mut CreateEmbed
where
    B: FnOnce(&mut CreateEmbed) -> &mut CreateEmbed,
{
    move |embed| {
        // First do our setup
        let embed = embed
            .author(|mut author| {
                if let Some(url) = avatar_url {
                    author = author.icon_url(url)
                }

                author
                    .name("Pomocop")
                    .url("https://github.com/nerosnm/pomocop")
            })
            .color(color)
            .footer(|footer| {
                footer.text(
                    "For support or suggestions, please click on the link in the title and file \
                     an issue",
                )
            });

        // Then let the caller change what they like
        builder(embed)
    }
}

/// Returns the URL of the current user's avatar, if it succeeded in being
/// found. If it couldn't be found, just returns `None` because I can't be
/// bothered.
async fn get_avatar_url(ctx: Context<'_>) -> Option<String> {
    ctx.discord()
        .http
        .get_current_user()
        .await
        .ok()
        .and_then(|user| user.avatar_url())
}

async fn send_reply<M>(ctx: Context<'_>, make_builder: M)
where
    M: for<'a, 'b> FnOnce(Option<String>, &'a mut CreateReply<'b>) -> &'a mut CreateReply<'b>,
{
    let avatar_url = get_avatar_url(ctx).await;

    let result = poise::send_reply(ctx, |reply| make_builder(avatar_url, reply)).await;

    if let Err(error) = result {
        error!(?error, "unable to send reply");
    }
}

async fn send_message<M>(ctx: Context<'_>, make_builder: M)
where
    M: for<'a, 'b> FnOnce(Option<String>, &'a mut CreateMessage<'b>) -> &'a mut CreateMessage<'b>,
{
    let avatar_url = get_avatar_url(ctx).await;

    let result = ctx
        .channel_id()
        .send_message(&ctx.discord().http, |message| {
            make_builder(avatar_url, message)
        })
        .await;

    if let Err(error) = result {
        error!(?error, "unable to send message");
    }
}

#[instrument(skip(ctx))]
pub async fn reply_starting(ctx: Context<'_>, config: &SessionConfig, id: Uuid) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(green_embed(avatar_url, |embed| {
            embed
                .title("Starting Session")
                .description(
                    "This session will run until the `/stop` command is used. Use `/skip` to skip \
                     the rest of the current phase and start the next one.",
                )
                .field("Work", format!("{} minutes", config.work), true)
                .field("Short Break", format!("{} minutes", config.short), true)
                .field("Long Break", format!("{} minutes", config.long), true)
                .field(
                    "Interval",
                    format!("Every {} work phases", config.interval),
                    false,
                )
                .field("Session ID", id, false)
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_cannot_start(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(red_embed(avatar_url, |embed| {
            embed.title("Unable to Start Session").description(
                "Only one session can be running in each channel at a time. Try running `/stop` \
                 to stop the running session, or run this command again in a different channel.",
            )
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn say_phase_finished(ctx: Context<'_>, finished: PhaseType, next: PhaseType) {
    send_message(ctx, |avatar_url, message| {
        message
            .content(MessageBuilder::new().mention(ctx.author()).build())
            .embed(green_embed(avatar_url, |embed| {
                embed
                    .title("Finished Phase")
                    .description(format!(
                        "Finished a {}. {}",
                        finished.description(),
                        finished.sentiment()
                    ))
                    .field("Starting Now", next.description(), false)
            }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_status(
    ctx: Context<'_>,
    phase_type: PhaseType,
    phase_elapsed: Duration,
    phase_remaining: Duration,
    next_type: PhaseType,
    long_at: DateTime<Utc>,
    tz: Tz,
) {
    send_reply(ctx, |avatar_url, reply| {
        reply
            .ephemeral(true)
            .embed(green_embed(avatar_url, |embed| {
                embed
                    .title("Status")
                    .field("Phase", phase_type.description(), false)
                    .field("Elapsed", phase_elapsed.hhmmss(), true)
                    .field("Remaining", phase_remaining.hhmmss(), true)
                    .field("Next", next_type.description(), true)
                    .field(
                        "Next Long Break",
                        format!(
                            "{} ({}), {} from now",
                            long_at.with_timezone(&tz).format("%H:%M:%S"),
                            tz,
                            (long_at - Utc::now()).hhmmss()
                        ),
                        false,
                    )
            }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_status_no_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.ephemeral(true).embed(red_embed(avatar_url, |embed| {
            embed.title("No Session").description(
                "Cannot get status because there is no running session in this channel.",
            )
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_skipping_phase(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(no_footer(green_embed(avatar_url, |embed| {
            embed.description("Skipping phase...")
        })))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_skip_failed(ctx: Context<'_>, id: Uuid) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(red_embed(avatar_url, |embed| {
            embed
                .title("Failed to Skip Phase")
                .description(formatdoc! { "
                    It may have completed on its own. Please check if the phase already advanced, and if not, try again.

                    A bug report would be appreciated. Please click on the link in the title of this embed, and quote the session ID below in your report. Thank you!
                    ",
                })
                .field("Session ID", id, false)
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_skip_no_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(red_embed(avatar_url, |embed| {
            embed
                .title("Failed to Skip Phase")
                .description("There is no running session in this channel!")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_stopping_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(no_footer(green_embed(avatar_url, |embed| {
            embed.description("Stopping session...")
        })))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_stop_failed(ctx: Context<'_>, id: Uuid) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(red_embed(avatar_url, |embed| {
            embed
                .title("Failed to Stop Session")
                .description(formatdoc! { "
                    Please try again.

                    A bug report would be appreciated. Please click on the link in the title of this embed, and quote the session ID below in your report. Thank you!
                    ",
                })
                .field("Session ID", id, false)
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_stop_no_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.embed(red_embed(avatar_url, |embed| {
            embed
                .title("Failed to Stop Session")
                .description("There is no running session in this channel!")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn say_session_stopped(ctx: Context<'_>) {
    send_message(ctx, |avatar_url, message| {
        message.embed(green_embed(avatar_url, |embed| {
            embed
                .title("Session Stopped")
                .description("Thanks for using Pomocop!")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn say_session_failed(ctx: Context<'_>, id: Uuid) {
    send_message(ctx, |avatar_url, message| {
        message.embed(red_embed(avatar_url, |embed| {
            embed
                .title("Session Failed")
                .description(
                    "Sorry about that! You can run `/start` to start a new session.

                    A bug report would be appreciated. Please click on the link in the title of \
                     this embed, and quote the session ID below in your report. Thank you!",
                )
                .field("Session ID", id, false)
        }))
    })
    .await;
}
