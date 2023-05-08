use std::ops::Deref;

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use hhmmss::Hhmmss;
use indoc::formatdoc;
use poise::{
    serenity_prelude::{self as serenity, CacheHttp},
    CreateReply,
};
use rand::seq::SliceRandom;
use serenity::{Color, CreateEmbed, CreateMessage, MessageBuilder, UserId};
use tracing::{error, instrument};
use uuid::Uuid;

use crate::{
    pomo::session::{PhaseType, SessionConfig},
    Context,
};

pub mod phrases;

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
    ctx.http()
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
        .send_message(&ctx.http(), |message| make_builder(avatar_url, message))
        .await;

    if let Err(error) = result {
        error!(?error, "unable to send message");
    }
}

#[instrument(skip(ctx))]
pub async fn reply_starting(ctx: Context<'_>, config: &SessionConfig, id: Uuid) {
    let mut rng = &mut *ctx.data().rng.lock().await;
    let phrase = phrases::STARTING_SESSION
        .choose(&mut rng)
        .expect("the list of phrases is not empty")
        .deref()
        .to_owned();

    send_reply(ctx, |avatar_url, reply| {
        reply
            .embed(green_embed(avatar_url, |embed| {
                embed
                    .title("Starting Session")
                    .description(formatdoc! { "
                        {}

                        This session will run until the `/stop` command is used. Use `/skip` to skip the rest of the current phase and start the next one.
                        ",
                        phrase
                    })
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
            embed.title("Unable to Start Session").description(formatdoc! {"
                Session is already running, now GET TO WORK.

                Only one session can be running in each channel at a time. Try running `/stop` to stop the running session, or run this command again in a different channel.
                 ",
            })
        }))
    })
    .await;
}

#[instrument(skip(ctx, members))]
pub async fn say_phase_finished<I, M>(
    ctx: Context<'_>,
    finished: PhaseType,
    next: PhaseType,
    members: I,
) where
    I: Iterator<Item = M>,
    M: AsRef<UserId>,
{
    let mentions = members
        .fold(&mut MessageBuilder::new(), |builder, member| {
            builder.mention(member.as_ref()).push(" ")
        })
        .build();

    let phrases = match next {
        PhaseType::Work(_) => phrases::STARTING_WORK,
        PhaseType::Short(_) => phrases::STARTING_SHORT_BREAK,
        PhaseType::Long(_) => phrases::STARTING_LONG_BREAK,
    };

    let mut rng = &mut *ctx.data().rng.lock().await;
    let phrase = phrases
        .choose(&mut rng)
        .expect("the list of phrases is not empty")
        .deref()
        .to_owned();

    send_message(ctx, |avatar_url, message| {
        message
            .content(mentions.trim())
            .embed(green_embed(avatar_url, |embed| {
                embed
                    .title(":rotating_light: WEE WOO :rotating_light: WEE WOO :rotating_light:")
                    .description(format!("Starting a {}. {}", next.description(), phrase))
                    .field("Just Finished", finished.description(), false)
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
            embed
                .title("No Session")
                .description("I can't tell you the status of a session that doesn't exist, genius.")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_joined(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply
            .ephemeral(true)
            .embed(green_embed(avatar_url, |embed| {
                embed.title("Session Joined").description(
                    "You will now be pinged when the phase changes. Use `/leave` to leave again.",
                )
            }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_join_already_member(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.ephemeral(true).embed(red_embed(avatar_url, |embed| {
            embed.title("Already a Member").description(
                "You are already a member of this session, idiot. Use `/leave` to leave.",
            )
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_join_no_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.ephemeral(true).embed(red_embed(avatar_url, |embed| {
            embed.title("No Session").description(
                "You can't join a session if there is no session! I can see you're paying \
                 attention...",
            )
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_left(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply
            .ephemeral(true)
            .embed(green_embed(avatar_url, |embed| {
                embed.title("Session Left").description(
                    "You will no longer be pinged when the phase changes. Use `/join` to join \
                     again.",
                )
            }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_leave_not_member(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.ephemeral(true).embed(red_embed(avatar_url, |embed| {
            embed.title("Not a Member").description(
                "You are not a member of this session, bird-brain. Use `/join` to join.",
            )
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_leave_no_session(ctx: Context<'_>) {
    send_reply(ctx, |avatar_url, reply| {
        reply.ephemeral(true).embed(red_embed(avatar_url, |embed| {
            embed
                .title("No Session")
                .description("Nice try, there has to be a session running for you to leave it.")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn reply_skipping_phase(ctx: Context<'_>, skipped: PhaseType) {
    let phrases = match skipped {
        PhaseType::Work(_) => phrases::SKIPPING_WORK,
        PhaseType::Short(_) | PhaseType::Long(_) => phrases::SKIPPING_BREAK,
    };

    let mut rng = &mut *ctx.data().rng.lock().await;
    let phrase = phrases
        .choose(&mut rng)
        .expect("the list of phrases is not empty")
        .deref()
        .to_owned();

    send_reply(ctx, |avatar_url, reply| {
        reply.embed(no_footer(green_embed(avatar_url, |embed| {
            embed.description(format!("Skipping {}. {}", skipped.description(), phrase))
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
            embed.title("Failed to Skip Phase").description(
                "I'm not even running a session and you're already trying to get out of work?",
            )
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
                .description("Trying to quit before you've even started?")
        }))
    })
    .await;
}

#[instrument(skip(ctx))]
pub async fn say_session_stopped(ctx: Context<'_>) {
    let mut rng = &mut *ctx.data().rng.lock().await;
    let phrase = phrases::STOPPING_SESSION
        .choose(&mut rng)
        .expect("the list of phrases is not empty")
        .deref()
        .to_owned();

    send_message(ctx, |avatar_url, message| {
        message.embed(green_embed(avatar_url, |embed| {
            embed.title("Session Stopped").description(phrase)
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
