use poise::builtins::HelpConfiguration;
use tracing::{info, instrument};

use crate::{Context, Error};

/// Show this help menu
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), Error> {
    info!("sending help");

    poise::builtins::help(
        ctx,
        command.as_deref(),
        HelpConfiguration {
            extra_text_at_bottom: "Pomocop is a Discord tomato timer bot that aims to be robust, \
                                   while also displaying the signature people-skills common to \
                                   law enforcement officers, VC-backed techbros and everyone's \
                                   least favourite teachers.",
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

/// Register application commands in this guild or globally
///
/// Run with no arguments to register in guild, run with argument "global" to
/// register globally.
#[instrument(skip(ctx))]
#[poise::command(prefix_command, check = "is_owner", hide_in_help)]
pub async fn register(ctx: Context<'_>, #[flag] global: bool) -> Result<(), Error> {
    info!("registering slash commands");

    poise::builtins::register_application_commands(ctx.into(), global).await?;

    Ok(())
}

pub async fn is_owner(ctx: Context<'_>) -> Result<bool, Error> {
    Ok(ctx.author().id == ctx.data().owner_id)
}
