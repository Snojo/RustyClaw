use crate::config::Config;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Debug, Clone)]
pub struct GatewayOptions {
    pub listen: String,
}

/// Run the gateway WebSocket server.
///
/// Accepts connections in a loop until the `cancel` token is triggered,
/// at which point the server shuts down gracefully.
pub async fn run_gateway(
    config: Config,
    options: GatewayOptions,
    cancel: CancellationToken,
) -> Result<()> {
    let addr = resolve_listen_addr(&options.listen)?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind gateway to {}", addr))?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break;
            }
            accepted = listener.accept() => {
                let (stream, peer) = accepted?;
                let config_clone = config.clone();
                let child_cancel = cancel.child_token();
                tokio::spawn(async move {
                    if let Err(err) = handle_connection(stream, peer, config_clone, child_cancel).await {
                        eprintln!("Gateway connection error from {}: {}", peer, err);
                    }
                });
            }
        }
    }

    Ok(())
}

fn resolve_listen_addr(listen: &str) -> Result<SocketAddr> {
    let trimmed = listen.trim();
    if trimmed.starts_with("ws://") || trimmed.starts_with("wss://") {
        let url = Url::parse(trimmed).context("Invalid WebSocket URL")?;
        let host = url.host_str().context("WebSocket URL missing host")?;
        let port = url
            .port_or_known_default()
            .context("WebSocket URL missing port")?;
        let addr = format!("{}:{}", host, port);
        return addr
            .parse()
            .with_context(|| format!("Invalid listen address {}", addr));
    }

    trimmed
        .parse()
        .with_context(|| format!("Invalid listen address {}", trimmed))
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    peer: SocketAddr,
    config: Config,
    cancel: CancellationToken,
) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .context("WebSocket handshake failed")?;
    let (mut writer, mut reader) = ws_stream.split();

    let hello = json!({
        "type": "hello",
        "agent": "rustyclaw",
        "settings_dir": config.settings_dir,
    });
    writer
        .send(Message::Text(hello.to_string()))
        .await
        .context("Failed to send hello message")?;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = writer.send(Message::Close(None)).await;
                break;
            }
            msg = reader.next() => {
                let message = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                };
                match message {
                    Message::Text(text) => {
                        let response = json!({
                            "type": "response",
                            "ok": true,
                            "peer": peer.to_string(),
                            "received": text,
                        });
                        writer
                            .send(Message::Text(response.to_string()))
                            .await
                            .context("Failed to send response")?;
                    }
                    Message::Binary(_) => {
                        let response = json!({
                            "type": "error",
                            "ok": false,
                            "message": "Binary frames are not supported",
                        });
                        writer
                            .send(Message::Text(response.to_string()))
                            .await
                            .context("Failed to send error response")?;
                    }
                    Message::Close(_) => {
                        break;
                    }
                    Message::Ping(payload) => {
                        writer.send(Message::Pong(payload)).await?;
                    }
                    Message::Pong(_) => {}
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
