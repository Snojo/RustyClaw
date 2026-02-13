use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Represents a message in the messenger system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub content: String,
    pub timestamp: i64,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// Trait for messenger implementations (OpenClaw compatible)
#[async_trait]
pub trait Messenger: Send + Sync {
    /// Get the messenger name
    fn name(&self) -> &str;

    /// Get the messenger type (telegram, discord, webhook, etc.)
    fn messenger_type(&self) -> &str;

    /// Initialize the messenger
    async fn initialize(&mut self) -> Result<()>;

    /// Send a message
    async fn send_message(&self, recipient: &str, content: &str) -> Result<String>;

    /// Send a message with options
    async fn send_message_with_options(&self, opts: SendOptions<'_>) -> Result<String> {
        // Default implementation ignores options
        self.send_message(opts.recipient, opts.content).await
    }

    /// Receive messages (non-blocking)
    async fn receive_messages(&self) -> Result<Vec<Message>>;

    /// Check if the messenger is connected
    fn is_connected(&self) -> bool;

    /// Disconnect the messenger
    async fn disconnect(&mut self) -> Result<()>;
}

/// Options for sending a message
#[derive(Debug, Default)]
pub struct SendOptions<'a> {
    pub recipient: &'a str,
    pub content: &'a str,
    pub reply_to: Option<&'a str>,
    pub silent: bool,
    pub media: Option<&'a str>,
}

/// Manager for multiple messengers
pub struct MessengerManager {
    messengers: Vec<Box<dyn Messenger>>,
}

impl MessengerManager {
    pub fn new() -> Self {
        Self {
            messengers: Vec::new(),
        }
    }

    /// Add a messenger to the manager
    pub fn add_messenger(&mut self, messenger: Box<dyn Messenger>) {
        self.messengers.push(messenger);
    }

    /// Initialize all messengers
    pub async fn initialize_all(&mut self) -> Result<()> {
        for messenger in &mut self.messengers {
            messenger.initialize().await?;
        }
        Ok(())
    }

    /// Get all messengers
    pub fn get_messengers(&self) -> &[Box<dyn Messenger>] {
        &self.messengers
    }

    /// Get a messenger by name
    pub fn get_messenger(&self, name: &str) -> Option<&dyn Messenger> {
        self.messengers
            .iter()
            .find(|m| m.name() == name)
            .map(|b| &**b)
    }

    /// Get a messenger by type
    pub fn get_messenger_by_type(&self, msg_type: &str) -> Option<&dyn Messenger> {
        self.messengers
            .iter()
            .find(|m| m.messenger_type() == msg_type)
            .map(|b| &**b)
    }

    /// Disconnect all messengers
    pub async fn disconnect_all(&mut self) -> Result<()> {
        for messenger in &mut self.messengers {
            messenger.disconnect().await?;
        }
        Ok(())
    }
}

impl Default for MessengerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Webhook Messenger ───────────────────────────────────────────────────────

/// Simple webhook messenger that POSTs messages to a URL
pub struct WebhookMessenger {
    name: String,
    webhook_url: String,
    connected: bool,
    http: reqwest::Client,
}

impl WebhookMessenger {
    pub fn new(name: String, webhook_url: String) -> Self {
        Self {
            name,
            webhook_url,
            connected: false,
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct WebhookPayload<'a> {
    content: &'a str,
    recipient: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<&'a str>,
}

#[async_trait]
impl Messenger for WebhookMessenger {
    fn name(&self) -> &str {
        &self.name
    }

    fn messenger_type(&self) -> &str {
        "webhook"
    }

    async fn initialize(&mut self) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn send_message(&self, recipient: &str, content: &str) -> Result<String> {
        let payload = WebhookPayload {
            content,
            recipient,
            reply_to: None,
        };

        let resp = self
            .http
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(format!("webhook-{}", chrono::Utc::now().timestamp_millis()))
        } else {
            anyhow::bail!("Webhook returned {}", resp.status())
        }
    }

    async fn send_message_with_options(&self, opts: SendOptions<'_>) -> Result<String> {
        let payload = WebhookPayload {
            content: opts.content,
            recipient: opts.recipient,
            reply_to: opts.reply_to,
        };

        let resp = self
            .http
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(format!("webhook-{}", chrono::Utc::now().timestamp_millis()))
        } else {
            anyhow::bail!("Webhook returned {}", resp.status())
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>> {
        // Webhooks are typically outbound-only
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }
}

// ── Console Messenger (for testing) ─────────────────────────────────────────

/// Console messenger that prints to stdout (useful for testing/debugging)
pub struct ConsoleMessenger {
    name: String,
    connected: bool,
}

impl ConsoleMessenger {
    pub fn new(name: String) -> Self {
        Self {
            name,
            connected: false,
        }
    }
}

#[async_trait]
impl Messenger for ConsoleMessenger {
    fn name(&self) -> &str {
        &self.name
    }

    fn messenger_type(&self) -> &str {
        "console"
    }

    async fn initialize(&mut self) -> Result<()> {
        self.connected = true;
        eprintln!("[ConsoleMessenger] Initialized");
        Ok(())
    }

    async fn send_message(&self, recipient: &str, content: &str) -> Result<String> {
        let id = format!("console-{}", chrono::Utc::now().timestamp_millis());
        println!("[{}] To {}: {}", self.name, recipient, content);
        Ok(id)
    }

    async fn receive_messages(&self) -> Result<Vec<Message>> {
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        eprintln!("[ConsoleMessenger] Disconnected");
        Ok(())
    }
}

// ── Discord Messenger (stub) ────────────────────────────────────────────────

/// Discord messenger using bot token and channel webhooks
pub struct DiscordMessenger {
    name: String,
    bot_token: String,
    connected: bool,
    http: reqwest::Client,
}

impl DiscordMessenger {
    pub fn new(name: String, bot_token: String) -> Self {
        Self {
            name,
            bot_token,
            connected: false,
            http: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Messenger for DiscordMessenger {
    fn name(&self) -> &str {
        &self.name
    }

    fn messenger_type(&self) -> &str {
        "discord"
    }

    async fn initialize(&mut self) -> Result<()> {
        // Verify bot token by fetching current user
        let resp = self
            .http
            .get("https://discord.com/api/v10/users/@me")
            .header("Authorization", format!("Bot {}", self.bot_token))
            .send()
            .await?;

        if resp.status().is_success() {
            self.connected = true;
            Ok(())
        } else {
            anyhow::bail!("Discord auth failed: {}", resp.status())
        }
    }

    async fn send_message(&self, channel_id: &str, content: &str) -> Result<String> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await?;
            Ok(data["id"].as_str().unwrap_or("unknown").to_string())
        } else {
            anyhow::bail!("Discord send failed: {}", resp.status())
        }
    }

    async fn receive_messages(&self) -> Result<Vec<Message>> {
        // Real implementation would use Discord gateway WebSocket
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }
}

// ── Telegram Messenger (stub) ───────────────────────────────────────────────

/// Telegram messenger using bot API
pub struct TelegramMessenger {
    name: String,
    bot_token: String,
    connected: bool,
    http: reqwest::Client,
}

impl TelegramMessenger {
    pub fn new(name: String, bot_token: String) -> Self {
        Self {
            name,
            bot_token,
            connected: false,
            http: reqwest::Client::new(),
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }
}

#[async_trait]
impl Messenger for TelegramMessenger {
    fn name(&self) -> &str {
        &self.name
    }

    fn messenger_type(&self) -> &str {
        "telegram"
    }

    async fn initialize(&mut self) -> Result<()> {
        // Verify bot token with getMe
        let resp = self.http.get(self.api_url("getMe")).send().await?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await?;
            if data["ok"].as_bool() == Some(true) {
                self.connected = true;
                return Ok(());
            }
        }
        anyhow::bail!("Telegram auth failed")
    }

    async fn send_message(&self, chat_id: &str, content: &str) -> Result<String> {
        let resp = self
            .http
            .post(self.api_url("sendMessage"))
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": content,
                "parse_mode": "Markdown"
            }))
            .send()
            .await?;

        if resp.status().is_success() {
            let data: serde_json::Value = resp.json().await?;
            if data["ok"].as_bool() == Some(true) {
                return Ok(data["result"]["message_id"].to_string());
            }
        }
        anyhow::bail!("Telegram send failed")
    }

    async fn receive_messages(&self) -> Result<Vec<Message>> {
        // Real implementation would use getUpdates or webhooks
        Ok(Vec::new())
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_messenger_manager_creation() {
        let manager = MessengerManager::new();
        assert_eq!(manager.get_messengers().len(), 0);
    }

    #[tokio::test]
    async fn test_console_messenger() {
        let mut messenger = ConsoleMessenger::new("test".to_string());
        messenger.initialize().await.unwrap();
        assert!(messenger.is_connected());

        let id = messenger.send_message("user", "hello").await.unwrap();
        assert!(id.starts_with("console-"));

        messenger.disconnect().await.unwrap();
        assert!(!messenger.is_connected());
    }

    #[test]
    fn test_webhook_messenger_creation() {
        let messenger = WebhookMessenger::new(
            "test-webhook".to_string(),
            "https://example.com/webhook".to_string(),
        );
        assert_eq!(messenger.name(), "test-webhook");
        assert_eq!(messenger.messenger_type(), "webhook");
    }
}
