use std::{collections::HashMap, sync::Mutex, time::Duration};

use poise::{
    defaults::HelpResponseMode, serenity_prelude as serenity, EditTracker, ErrorContext, Framework,
    FrameworkOptions, PrefixFrameworkOptions,
};
use serenity::{ApplicationId, UserId};
use tracing::{info, instrument};

// Types used by all command functions
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;
pub type PrefixContext<'a> = poise::PrefixContext<'a, Data, Error>;

// Custom user data passed to all command functions
pub struct Data {
    pub _votes: Mutex<HashMap<String, u32>>,
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

    options.command(help(), |f| f);
    options.command(register(), |f| f);

    let framework = Framework::new(
        prefix,
        ApplicationId(application_id.parse()?),
        move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    _votes: Mutex::new(HashMap::new()),
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

/// Show this help menu
#[instrument(skip(ctx))]
#[poise::command(track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), Error> {
    info!("sending help");

    poise::defaults::help(
        ctx,
        command.as_deref(),
        "Pomocop is a tomato timer bot that isn't perpetually scuffed",
        HelpResponseMode::Ephemeral,
    )
    .await?;
    Ok(())
}

/// Register slash commands in this guild or globally
///
/// Run with no arguments to register in guild, run with argument "global" to
/// register globally.
#[instrument(skip(ctx))]
#[poise::command(check = "is_owner", hide_in_help)]
pub async fn register(ctx: PrefixContext<'_>, #[flag] global: bool) -> Result<(), Error> {
    info!("registering slash commands");

    poise::defaults::register_slash_commands(ctx, global).await?;

    Ok(())
}

pub async fn is_owner(ctx: PrefixContext<'_>) -> Result<bool, Error> {
    Ok(ctx.msg.author.id == ctx.data.owner_id)
}

pub async fn on_error(error: Error, ctx: ErrorContext<'_, Data, Error>) {
    match ctx {
        ErrorContext::Setup => panic!("Failed to start bot: {:?}", error),
        ErrorContext::Command(ctx) => {
            println!("Error in command `{}`: {:?}", ctx.command().name(), error)
        }
        _ => println!("Other error: {:?}", error),
    }
}
