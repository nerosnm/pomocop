use poise::defaults::HelpResponseMode;
use tracing::{info, instrument};

use crate::{Context, Error, PrefixContext};

/// Show this help menu
#[instrument(skip(ctx))]
#[poise::command(slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), Error> {
    info!("sending help");

    poise::defaults::help(
        ctx,
        command.as_deref(),
        "Pomocop is a Discord tomato timer bot that aims to be robust, while also displaying the \
         signature people-skills common to law enforcement officers, VC-backed techbros and \
         everyone's least favourite teachers.",
        HelpResponseMode::Ephemeral,
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
pub async fn register(ctx: PrefixContext<'_>, #[flag] global: bool) -> Result<(), Error> {
    info!("registering slash commands");

    poise::defaults::register_application_commands(ctx.into(), global).await?;

    Ok(())
}

pub async fn is_owner(ctx: PrefixContext<'_>) -> Result<bool, Error> {
    Ok(ctx.msg.author.id == ctx.data.owner_id)
}
