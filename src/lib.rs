use std::{collections::HashMap, time::Duration};

use poise::{
    serenity_prelude::{self as serenity, GatewayIntents, UserId},
    EditTracker, FrameworkBuilder, FrameworkError, FrameworkOptions, PrefixFrameworkOptions,
};
use rand::{rngs::StdRng, thread_rng, SeedableRng};
use serenity::ChannelId;
use tokio::sync::Mutex;
use tracing::{error, info, instrument};

use crate::pomo::session::Session;

pub mod commands;
pub mod pomo;

// Types used by all command functions
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub type PrefixContext<'a> = poise::PrefixContext<'a, Data, Error>;

// Custom user data passed to all command functions
#[derive(Debug)]
pub struct Data {
    pub sessions: Mutex<HashMap<ChannelId, Session>>,
    pub rng: Mutex<StdRng>,
    pub owner_id: serenity::UserId,
}

#[instrument(skip(token))]
pub async fn run(
    _application_id: String,
    owner_id: String,
    prefix: String,
    token: String,
) -> Result<(), Error> {
    info!("starting pomocop");

    let options = FrameworkOptions {
        prefix_options: PrefixFrameworkOptions {
            prefix: Some(prefix),
            edit_tracker: Some(EditTracker::for_timespan(Duration::from_secs(3600))),
            ..Default::default()
        },
        on_error: |error| Box::pin(on_error(error)),
        commands: vec![
            commands::meta::help(),
            commands::meta::register(),
            commands::pomo::start(),
            commands::pomo::status(),
            commands::pomo::join(),
            commands::pomo::leave(),
            commands::pomo::skip(),
            commands::pomo::stop(),
        ],
        ..Default::default()
    };

    let framework = FrameworkBuilder::<Data, Error>::default()
        .options(options)
        .token(token)
        .intents(GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT)
        .setup(move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    sessions: Mutex::new(HashMap::new()),
                    rng: Mutex::new(
                        StdRng::from_rng(thread_rng())
                            .expect("unable to seed StdRng from ThreadRng"),
                    ),
                    owner_id: UserId(owner_id.parse()?),
                })
            })
        })
        .build()
        .await?;

    framework.start().await?;

    Ok(())
}

pub async fn on_error(error: FrameworkError<'_, Data, Error>) {
    match error {
        FrameworkError::Command { ctx, .. } => {
            error!(?error, command = %ctx.command().name, "error in command")
        }
        FrameworkError::Setup { .. } => panic!("failed to start bot: {:?}", error),
        _ => error!("other error: {:?}", error),
    }
}
