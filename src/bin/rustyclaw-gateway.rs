use anyhow::Result;
use clap::{Parser, Subcommand};
use rustyclaw::args::CommonArgs;
use rustyclaw::config::Config;
use rustyclaw::gateway::{run_gateway, GatewayOptions};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Parser)]
#[command(name = "rustyclaw-gateway", version, about = "RustyClaw gateway")]
struct GatewayCli {
    #[command(flatten)]
    common: CommonArgs,
    #[command(subcommand)]
    command: Option<GatewayCommands>,
}

#[derive(Debug, Subcommand)]
enum GatewayCommands {
    Serve(ServeArgs),
}

#[derive(Debug, clap::Args)]
struct ServeArgs {
    /// WebSocket listen URL (ws://host:port) or host:port
    #[arg(long = "listen", alias = "url", alias = "ws", value_name = "WS_URL")]
    listen: Option<String>,
    /// Bind host (used if --listen is not provided)
    #[arg(long, value_name = "HOST", default_value = "127.0.0.1")]
    host: String,
    /// Bind port (used if --listen is not provided)
    #[arg(long, value_name = "PORT", default_value_t = 9001)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = GatewayCli::parse();
    let config_path = cli.common.config_path();
    let mut config = Config::load(config_path)?;
    cli.common.apply_overrides(&mut config);

    let args = match cli.command {
        Some(GatewayCommands::Serve(args)) => args,
        None => ServeArgs {
            listen: None,
            host: "127.0.0.1".to_string(),
            port: 9001,
        },
    };

    let listen = args
        .listen
        .unwrap_or_else(|| format!("{}:{}", args.host, args.port));

    println!("RustyClaw gateway listening on ws://{}", listen);

    // The standalone binary runs until interrupted (no cancellation token needed).
    let cancel = CancellationToken::new();
    run_gateway(config, GatewayOptions { listen }, cancel).await
}
