# Pomocop

Pomocop is a Discord tomato timer bot that aims to be robust, while also displaying the signature 
people-skills common to law enforcement officers, VC-backed techbros and everyone's least favourite 
teachers.

## Usage

Here is a summary of the available slash commands:

- `/start`: Start a tomato timer session in the Discord channel or DM the command is run in. By 
   default, the session will comprise 25 minute work sessions followed by 5 minute short breaks, 
   except for every 4th break, which is a 15 minute long break. Each of those values is 
   customisable.
- `/stop`: Stop the session.
- `/skip`: Skip the current phase (work session or break) and start the next one.
- `/status`: Get some information about the current status of the session.
- `/join`: Subscribe to mentions from the bot each time the phase changes, for the currently running 
    session in the channel the command is run in.
- `/leave`: Unsubscribe from phase change notifications.
- `/help`: Get information about available commands.

## Running the Bot

### Build

#### Using `cargo`

First, make sure you have `cargo` and `rustc` installed, ideally through [`rustup`][rustup]. Then 
run `cargo build --release`.

#### Using Nix

This repository contains a [Nix flake][flakes]. Just run `nix build github:nerosnm/pomocop/main`.

### Configuration

Create a file `.env`, in the following format (you can copy `.env.sample` if you want to):

```
APPLICATION_ID=""
OWNER_ID=""
TOKEN=""
```

- Set `APPLICATION_ID` and `TOKEN` to values obtained by registering an application in [the Discord 
    Developer portal][dev-portal]. Make sure you add a bot user!
- Set `OWNER_ID` to your user account's ID (you can get this by right-clicking your name in Discord 
    and selecting "Copy ID").

### Run

Run `pomocop` from the same directory as the `.env` file you just created.

You may want to set some additional environment variables (these can be set by adding them to `.env` 
if you want):

- `PREFIX`: The default prefix for non-slash commands is `|`, but you can change this by setting 
    `PREFIX` to some other string.
- `RUST_LOG`: See the [`tracing-subscriber` docs][sub] for details about setting this value. I would 
    recommend `info` or `info,pomocop=debug`.

### Setup

Invite your bot to a server using this link, replacing `<client_id>` with the Client ID of your app:

`https://discord.com/api/oauth2/authorize?client_id=<client_id>&permissions=3198016&scope=bot%20applications.commands`

Once your bot is in a server, run `|register global` to globally register the bot's slash commands. 
This can take some time to update, so you can run `|register` to register the commands only in the 
server you that command is run in, but keep in mind this can result in duplicate slash commands 
showing up (if this happens, kick the bot from your server and invite it again).

[rustup]: https://rustup.rs
[flakes]: https://nixos.wiki/wiki/Flakes
[dev-portal]: https://discord.com/developers
[sub]: https://docs.rs/tracing-subscriber/0.2.15/tracing_subscriber/fmt/index.html#filtering-events-with-environment-variables
