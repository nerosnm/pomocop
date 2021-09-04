use std::{collections::HashMap, sync::Mutex, time::Duration};

use poise::{
    serenity_prelude as serenity, EditTracker, ErrorContext, Framework, FrameworkOptions,
    PrefixFrameworkOptions,
};
use serenity::{ApplicationId, ChannelId, UserId};
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::pomo::Session;

pub mod commands;
pub mod pomo;

// Types used by all command functions
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub type PrefixContext<'a> = poise::PrefixContext<'a, Data, Error>;

// Custom user data passed to all command functions
pub struct Data {
    pub sessions: Mutex<HashMap<ChannelId, Session>>,
    pub owner_id: serenity::UserId,
}

#[instrument(skip(token))]
pub async fn start(
    application_id: String,
    owner_id: String,
    prefix: String,
    token: String,
) -> Result<(), Error> {
    info!("starting pomocop");

    let mut options = FrameworkOptions {
        prefix_options: PrefixFrameworkOptions {
            edit_tracker: Some(EditTracker::for_timespan(Duration::from_secs(3600))),
            ..Default::default()
        },
        on_error: |error, ctx| Box::pin(on_error(error, ctx)),
        ..Default::default()
    };

    options.command(commands::meta::help(), |f| f);
    options.command(commands::meta::register(), |f| f);
    // options.command(commands::pomo::start(), |f| f);
    // options.command(commands::pomo::stop(), |f| f);

    let framework = Framework::new(
        prefix,
        ApplicationId(application_id.parse()?),
        move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    sessions: Mutex::new(HashMap::new()),
                    owner_id: UserId(owner_id.parse()?),
                })
            })
        },
        options,
    );
    framework
        .start(serenity::ClientBuilder::new(&token))
        .await?;

    Ok(())
}

pub async fn on_error(error: Error, ctx: ErrorContext<'_, Data, Error>) {
    match ctx {
        ErrorContext::Setup => panic!("failed to start bot: {:?}", error),
        ErrorContext::Command(ctx) => {
            error!(?error, command = %ctx.command().name(), "error in command")
        }
        _ => error!(?error, "other error"),
    }
}
