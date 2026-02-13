//! Messenger integration for the gateway.
//!
//! This module provides the messenger polling loop that receives messages
//! from configured messengers (Telegram, Discord, Signal, etc.) and routes
//! them through the model for processing.

use crate::config::{Config, MessengerConfig};
use crate::messengers::{
    DiscordMessenger, Message, Messenger, MessengerManager, SendOptions, TelegramMessenger,
    WebhookMessenger,
};
use crate::tools;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::{ChatMessage, ModelContext, SharedSkillManager, SharedVault};

#[cfg(feature = "matrix")]
use crate::messengers::MatrixMessenger;

#[cfg(feature = "signal")]
use crate::messengers::SignalMessenger;

/// Shared messenger manager for the gateway.
pub type SharedMessengerManager = Arc<Mutex<MessengerManager>>;

/// Create a messenger manager from config.
pub async fn create_messenger_manager(config: &Config) -> Result<MessengerManager> {
    let mut manager = MessengerManager::new();

    for messenger_config in &config.messengers {
        match create_messenger(messenger_config).await {
            Ok(messenger) => {
                eprintln!(
                    "[messenger] Initialized {} ({})",
                    messenger.name(),
                    messenger.messenger_type()
                );
                manager.add_messenger(messenger);
            }
            Err(e) => {
                eprintln!(
                    "[messenger] Failed to initialize {}: {}",
                    messenger_config.messenger_type, e
                );
            }
        }
    }

    Ok(manager)
}

/// Create a single messenger from config.
async fn create_messenger(config: &MessengerConfig) -> Result<Box<dyn Messenger>> {
    let mut messenger: Box<dyn Messenger> = match config.messenger_type.as_str() {
        "telegram" => {
            let token = config
                .token
                .clone()
                .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
                .context("Telegram requires 'token' or TELEGRAM_BOT_TOKEN env var")?;
            Box::new(TelegramMessenger::new(&token))
        }
        "discord" => {
            let token = config
                .token
                .clone()
                .or_else(|| std::env::var("DISCORD_BOT_TOKEN").ok())
                .context("Discord requires 'token' or DISCORD_BOT_TOKEN env var")?;
            Box::new(DiscordMessenger::new(&token))
        }
        "webhook" => {
            let url = config
                .webhook_url
                .clone()
                .or_else(|| std::env::var("WEBHOOK_URL").ok())
                .context("Webhook requires 'webhook_url' or WEBHOOK_URL env var")?;
            Box::new(WebhookMessenger::new(&url))
        }
        #[cfg(feature = "matrix")]
        "matrix" => {
            let homeserver = config
                .homeserver
                .clone()
                .context("Matrix requires 'homeserver'")?;
            let user_id = config.user_id.clone().context("Matrix requires 'user_id'")?;
            let password = config.password.clone();
            let access_token = config.access_token.clone();

            let messenger = MatrixMessenger::new(&homeserver, &user_id, password, access_token)
                .context("Failed to create Matrix messenger")?;
            Box::new(messenger)
        }
        #[cfg(feature = "signal")]
        "signal" => {
            let phone = config
                .phone
                .clone()
                .context("Signal requires 'phone' number")?;
            let messenger =
                SignalMessenger::new(&phone).context("Failed to create Signal messenger")?;
            Box::new(messenger)
        }
        other => anyhow::bail!("Unknown messenger type: {}", other),
    };

    messenger.initialize().await?;
    Ok(messenger)
}

/// Run the messenger polling loop.
///
/// This polls all configured messengers for incoming messages and routes
/// them through the model for processing. Responses are sent back via
/// the originating messenger.
pub async fn run_messenger_loop(
    config: Config,
    messenger_mgr: SharedMessengerManager,
    model_ctx: Option<Arc<ModelContext>>,
    vault: SharedVault,
    skill_mgr: SharedSkillManager,
    cancel: CancellationToken,
) -> Result<()> {
    // If no model context, we can't process messages
    let model_ctx = match model_ctx {
        Some(ctx) => ctx,
        None => {
            eprintln!("[messenger] No model context — messenger loop disabled");
            return Ok(());
        }
    };

    let poll_interval = Duration::from_millis(
        config
            .messenger_poll_interval_ms
            .unwrap_or(2000)
            .max(500) as u64,
    );

    eprintln!(
        "[messenger] Starting messenger loop (poll interval: {:?})",
        poll_interval
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                eprintln!("[messenger] Shutting down messenger loop");
                break;
            }
            _ = tokio::time::sleep(poll_interval) => {
                // Poll all messengers for incoming messages
                let messages = {
                    let mgr = messenger_mgr.lock().await;
                    poll_all_messengers(&mgr).await
                };

                // Process each message
                for (messenger_type, msg) in messages {
                    if let Err(e) = process_incoming_message(
                        &config,
                        &messenger_mgr,
                        &model_ctx,
                        &vault,
                        &skill_mgr,
                        &messenger_type,
                        msg,
                    )
                    .await
                    {
                        eprintln!("[messenger] Error processing message: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Poll all messengers and collect incoming messages.
async fn poll_all_messengers(mgr: &MessengerManager) -> Vec<(String, Message)> {
    let mut all_messages = Vec::new();

    for messenger in mgr.get_messengers() {
        match messenger.receive_messages().await {
            Ok(messages) => {
                for msg in messages {
                    all_messages.push((messenger.messenger_type().to_string(), msg));
                }
            }
            Err(e) => {
                eprintln!(
                    "[messenger] Error polling {}: {}",
                    messenger.messenger_type(),
                    e
                );
            }
        }
    }

    all_messages
}

/// Process an incoming message through the model and send response.
async fn process_incoming_message(
    config: &Config,
    messenger_mgr: &SharedMessengerManager,
    model_ctx: &Arc<ModelContext>,
    vault: &SharedVault,
    skill_mgr: &SharedSkillManager,
    messenger_type: &str,
    msg: Message,
) -> Result<()> {
    eprintln!(
        "[messenger] Received from {} ({}): {}",
        msg.sender,
        messenger_type,
        if msg.content.len() > 50 {
            format!("{}...", &msg.content[..50])
        } else {
            msg.content.clone()
        }
    );

    // Build context for the model
    let workspace_dir = config.workspace_dir();

    // Create a simple conversation history with just this message
    let history = vec![super::ChatMessage {
        role: "user".to_string(),
        content: Some(msg.content.clone()),
        tool_calls: None,
        tool_call_id: None,
    }];

    // Prepare system prompt with messenger context
    let system_prompt = build_messenger_system_prompt(config, messenger_type, &msg);

    // Get response from model
    // Note: dispatch_text_message is designed for WebSocket streaming,
    // we need a simpler version for messengers
    let response = call_model_simple(model_ctx, &system_prompt, &history, vault, skill_mgr, &workspace_dir).await?;

    // Send response back via messenger
    if !response.is_empty() && response != "NO_REPLY" && response != "HEARTBEAT_OK" {
        let mgr = messenger_mgr.lock().await;
        if let Some(messenger) = mgr.get_messenger_by_type(messenger_type) {
            // Determine recipient (channel or sender)
            let recipient = msg.channel.as_deref().unwrap_or(&msg.sender);

            let opts = SendOptions {
                recipient,
                content: &response,
                reply_to: Some(&msg.id),
                silent: false,
                media: None,
            };

            match messenger.send_message_with_options(opts).await {
                Ok(msg_id) => {
                    eprintln!("[messenger] Sent response ({}): {}", msg_id, 
                        if response.len() > 50 { format!("{}...", &response[..50]) } else { response.clone() }
                    );
                }
                Err(e) => {
                    eprintln!("[messenger] Failed to send response: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Build system prompt with messenger context.
fn build_messenger_system_prompt(config: &Config, messenger_type: &str, msg: &Message) -> String {
    let base_prompt = config.system_prompt.clone().unwrap_or_else(|| {
        "You are a helpful AI assistant.".to_string()
    });

    format!(
        "{}\n\n## Messaging Context\n\
        - Channel: {}\n\
        - Sender: {}\n\
        - Platform: {}\n\
        \n\
        When responding, be concise and appropriate for chat. \
        If you have nothing to say, reply with: NO_REPLY",
        base_prompt,
        msg.channel.as_deref().unwrap_or("direct"),
        msg.sender,
        messenger_type
    )
}

/// Simple model call without WebSocket streaming.
async fn call_model_simple(
    model_ctx: &Arc<ModelContext>,
    system_prompt: &str,
    history: &[super::ChatMessage],
    _vault: &SharedVault,
    _skill_mgr: &SharedSkillManager,
    _workspace_dir: &std::path::Path,
) -> Result<String> {
    use crate::providers;

    // Build messages array
    let mut messages: Vec<Value> = vec![json!({
        "role": "system",
        "content": system_prompt
    })];

    for msg in history {
        messages.push(json!({
            "role": msg.role,
            "content": msg.content
        }));
    }

    // Get tools
    let all_tools = tools::all_tools();
    let tool_defs: Vec<Value> = all_tools
        .iter()
        .map(|t| tools::to_openai_function(t))
        .collect();

    // Call provider
    let response = providers::chat_completion(
        &model_ctx.provider,
        model_ctx.api_key.as_deref(),
        &model_ctx.model,
        &messages,
        Some(&tool_defs),
        model_ctx.max_tokens,
        model_ctx.temperature,
    )
    .await?;

    // Extract text response
    // Note: This simplified version doesn't handle tool calls
    // A full implementation would need the agentic loop
    if let Some(choices) = response.get("choices").and_then(|c| c.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(content) = first.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_str()) {
                return Ok(content.to_string());
            }
        }
    }

    // Anthropic format
    if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
        let text_parts: Vec<&str> = content
            .iter()
            .filter_map(|c| {
                if c.get("type").and_then(|t| t.as_str()) == Some("text") {
                    c.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect();
        if !text_parts.is_empty() {
            return Ok(text_parts.join("\n"));
        }
    }

    Ok(String::new())
}
