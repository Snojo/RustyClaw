//! Messenger integration for the gateway.
//!
//! This module provides the messenger polling loop that receives messages
//! from configured messengers (Telegram, Discord, Signal, etc.) and routes
//! them through the model for processing with full tool loop support.

use crate::config::{Config, MessengerConfig};
use crate::messengers::{
    DiscordMessenger, Message, Messenger, MessengerManager, SendOptions, TelegramMessenger,
    WebhookMessenger,
};
use crate::tools;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::providers;
use super::secrets_handler;
use super::skills_handler;
use super::{ChatMessage, ModelContext, ProviderRequest, SharedSkillManager, SharedVault, ToolCallResult};

#[cfg(feature = "matrix")]
use crate::messengers::MatrixMessenger;

#[cfg(feature = "signal")]
use crate::messengers::SignalMessenger;

/// Shared messenger manager for the gateway.
pub type SharedMessengerManager = Arc<Mutex<MessengerManager>>;

/// Conversation history storage per chat.
/// Key: "messenger_type:chat_id" or "messenger_type:sender_id"
type ConversationStore = Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>;

/// Maximum messages to keep in conversation history per chat.
const MAX_HISTORY_MESSAGES: usize = 50;

/// Maximum tool loop rounds.
const MAX_TOOL_ROUNDS: usize = 25;

/// Create a messenger manager from config.
pub async fn create_messenger_manager(config: &Config) -> Result<MessengerManager> {
    let mut manager = MessengerManager::new();

    for messenger_config in &config.messengers {
        if !messenger_config.enabled {
            continue;
        }
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
/// them through the model for processing with full tool support.
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

    // Per-chat conversation history
    let conversations: ConversationStore = Arc::new(Mutex::new(HashMap::new()));

    let http = reqwest::Client::new();

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
                        &http,
                        &config,
                        &messenger_mgr,
                        &model_ctx,
                        &vault,
                        &skill_mgr,
                        &conversations,
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

/// Process an incoming message through the model with full tool loop.
async fn process_incoming_message(
    http: &reqwest::Client,
    config: &Config,
    messenger_mgr: &SharedMessengerManager,
    model_ctx: &Arc<ModelContext>,
    vault: &SharedVault,
    skill_mgr: &SharedSkillManager,
    conversations: &ConversationStore,
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

    let workspace_dir = config.workspace_dir();

    // Build conversation key for this chat
    let conv_key = format!(
        "{}:{}",
        messenger_type,
        msg.channel.as_deref().unwrap_or(&msg.sender)
    );

    // Get or create conversation history
    let mut messages = {
        let mut store = conversations.lock().await;
        store.entry(conv_key.clone()).or_insert_with(Vec::new).clone()
    };

    // Build system prompt
    let system_prompt = build_messenger_system_prompt(config, messenger_type, &msg);

    // Add system message if not present
    if messages.is_empty() || messages[0].role != "system" {
        messages.insert(
            0,
            ChatMessage {
                role: "system".to_string(),
                content: Some(system_prompt.clone()),
                tool_calls: None,
                tool_call_id: None,
            },
        );
    } else {
        // Update system prompt
        messages[0].content = Some(system_prompt.clone());
    }

    // Add user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: Some(msg.content.clone()),
        tool_calls: None,
        tool_call_id: None,
    });

    // Convert to provider format
    let provider_messages: Vec<Value> = messages
        .iter()
        .map(|m| {
            let mut obj = json!({
                "role": m.role,
            });
            if let Some(content) = &m.content {
                obj["content"] = json!(content);
            }
            if let Some(tool_calls) = &m.tool_calls {
                obj["tool_calls"] = tool_calls.clone();
            }
            if let Some(tool_call_id) = &m.tool_call_id {
                obj["tool_call_id"] = json!(tool_call_id);
            }
            obj
        })
        .collect();

    // Get tools
    let all_tools = tools::all_tools();
    let tool_defs: Vec<Value> = all_tools
        .iter()
        .map(|t| tools::to_openai_function(t))
        .collect();

    // Build request
    let mut resolved = ProviderRequest {
        provider: model_ctx.provider.clone(),
        model: model_ctx.model.clone(),
        api_key: model_ctx.api_key.clone(),
        messages: provider_messages,
        tools: Some(tool_defs),
        max_tokens: model_ctx.max_tokens,
        temperature: model_ctx.temperature,
        thinking: None,
    };

    // Run the agentic tool loop
    let mut final_response = String::new();

    for _round in 0..MAX_TOOL_ROUNDS {
        let result = if resolved.provider == "anthropic" {
            providers::call_anthropic_with_tools(http, &resolved).await
        } else if resolved.provider == "google" {
            providers::call_google_with_tools(http, &resolved).await
        } else {
            providers::call_openai_with_tools(http, &resolved).await
        };

        let model_resp = match result {
            Ok(r) => r,
            Err(err) => {
                eprintln!("[messenger] Model error: {}", err);
                return Err(err);
            }
        };

        // Collect text response
        if !model_resp.text.is_empty() {
            final_response.push_str(&model_resp.text);
        }

        if model_resp.tool_calls.is_empty() {
            // No tool calls — done
            break;
        }

        // Execute each requested tool
        let mut tool_results: Vec<ToolCallResult> = Vec::new();

        for tc in &model_resp.tool_calls {
            eprintln!("[messenger] Tool call: {} ({})", tc.name, tc.id);

            let (output, is_error) = if tools::is_secrets_tool(&tc.name) {
                match secrets_handler::execute_secrets_tool(&tc.name, &tc.arguments, vault).await {
                    Ok(text) => (text, false),
                    Err(err) => (err, true),
                }
            } else if tools::is_skill_tool(&tc.name) {
                match skills_handler::execute_skill_tool(&tc.name, &tc.arguments, skill_mgr).await {
                    Ok(text) => (text, false),
                    Err(err) => (err, true),
                }
            } else {
                match tools::execute_tool(&tc.name, &tc.arguments, &workspace_dir) {
                    Ok(text) => (text, false),
                    Err(err) => (err, true),
                }
            };

            eprintln!(
                "[messenger] Tool result ({}): {}",
                if is_error { "error" } else { "ok" },
                if output.len() > 100 {
                    format!("{}...", &output[..100])
                } else {
                    output.clone()
                }
            );

            tool_results.push(ToolCallResult {
                id: tc.id.clone(),
                name: tc.name.clone(),
                output,
                is_error,
            });
        }

        // Append tool round to conversation
        providers::append_tool_round(
            &resolved.provider,
            &mut resolved.messages,
            &model_resp,
            &tool_results,
        );
    }

    // Update conversation history
    {
        let mut store = conversations.lock().await;
        let history = store.entry(conv_key).or_insert_with(Vec::new);

        // Add user message
        history.push(ChatMessage {
            role: "user".to_string(),
            content: Some(msg.content.clone()),
            tool_calls: None,
            tool_call_id: None,
        });

        // Add assistant response
        if !final_response.is_empty() {
            history.push(ChatMessage {
                role: "assistant".to_string(),
                content: Some(final_response.clone()),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Trim history if too long (keep system message)
        while history.len() > MAX_HISTORY_MESSAGES {
            if history.len() > 1 && history[1].role != "system" {
                history.remove(1);
            } else {
                break;
            }
        }
    }

    // Send response back via messenger
    if !final_response.is_empty()
        && final_response.trim() != "NO_REPLY"
        && final_response.trim() != "HEARTBEAT_OK"
    {
        let mgr = messenger_mgr.lock().await;
        if let Some(messenger) = mgr.get_messenger_by_type(messenger_type) {
            let recipient = msg.channel.as_deref().unwrap_or(&msg.sender);

            let opts = SendOptions {
                recipient,
                content: &final_response,
                reply_to: Some(&msg.id),
                silent: false,
                media: None,
            };

            match messenger.send_message_with_options(opts).await {
                Ok(msg_id) => {
                    eprintln!(
                        "[messenger] Sent response ({}): {}",
                        msg_id,
                        if final_response.len() > 50 {
                            format!("{}...", &final_response[..50])
                        } else {
                            final_response.clone()
                        }
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
        When responding:\n\
        - Be concise and appropriate for chat\n\
        - You have access to tools — use them when helpful\n\
        - If you have nothing to say, reply with: NO_REPLY",
        base_prompt,
        msg.channel.as_deref().unwrap_or("direct"),
        msg.sender,
        messenger_type
    )
}
