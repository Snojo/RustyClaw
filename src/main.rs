use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use rustyclaw::app::App;
use rustyclaw::args::CommonArgs;
use rustyclaw::commands::{handle_command, CommandAction, CommandContext};
use rustyclaw::config::Config;
use rustyclaw::secrets::SecretsManager;
use rustyclaw::skills::SkillManager;
use tokio_tungstenite::tungstenite::Message;
use url::Url;

#[derive(Debug, Parser)]
#[command(name = "rustyclaw", version, about = "RustyClaw CLI")]
struct Cli {
    #[command(flatten)]
    common: CommonArgs,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(alias = "ui")]
    Tui(TuiArgs),
    #[command(alias = "cmd", alias = "run")]
    Command(CommandArgs),
}

#[derive(Debug, Args, Default)]
struct TuiArgs {}

#[derive(Debug, Args)]
struct CommandArgs {
    /// Command text to execute
    #[arg(value_name = "COMMAND", trailing_var_arg = true)]
    command: Vec<String>,
    /// Gateway WebSocket URL (ws://...)
    #[arg(long = "gateway", alias = "url", alias = "ws", value_name = "WS_URL", env = "RUSTYCLAW_GATEWAY")]
    gateway: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config_path = cli.common.config_path();
    let mut config = Config::load(config_path)?;
    cli.common.apply_overrides(&mut config);

    match cli.command.unwrap_or(Commands::Tui(TuiArgs::default())) {
        Commands::Tui(_) => {
            let mut app = App::new(config)?;
            app.run().await?;
        }
        Commands::Command(args) => {
            let input = args.command.join(" ").trim().to_string();
            if input.is_empty() {
                anyhow::bail!("No command provided.");
            }

            if let Some(gateway_url) = args.gateway {
                let response = send_command_via_gateway(&gateway_url, &input).await?;
                println!("{}", response);
            } else {
                run_local_command(&config, &input)?;
            }
        }
    }

    Ok(())
}

fn run_local_command(config: &Config, input: &str) -> Result<()> {
    let mut secrets_manager = SecretsManager::new(&config.settings_dir);
    let skills_dir = config
        .skills_dir
        .clone()
        .unwrap_or_else(|| config.settings_dir.join("skills"));
    let mut skill_manager = SkillManager::new(skills_dir);
    skill_manager.load_skills()?;

    let mut context = CommandContext {
        secrets_manager: &mut secrets_manager,
        skill_manager: &mut skill_manager,
    };

    let response = handle_command(input, &mut context);
    if response.action == CommandAction::ClearMessages {
        for message in response.messages {
            println!("{}", message);
        }
        return Ok(());
    }

    if response.action == CommandAction::Quit {
        return Ok(());
    }

    for message in response.messages {
        println!("{}", message);
    }

    Ok(())
}

async fn send_command_via_gateway(gateway_url: &str, command: &str) -> Result<String> {
    let url = Url::parse(gateway_url).context("Invalid gateway URL")?;

    let (ws_stream, _) = tokio_tungstenite::connect_async(url)
        .await
        .context("Failed to connect to gateway")?;
    let (mut writer, mut reader) = ws_stream.split();
    writer
        .send(Message::Text(command.to_string()))
        .await
        .context("Failed to send command")?;

    while let Some(message) = reader.next().await {
        let message = message.context("Gateway read error")?;
        if let Message::Text(text) = message {
            return Ok(text);
        }
    }

    anyhow::bail!("Gateway closed without responding")
}
