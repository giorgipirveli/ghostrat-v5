/**
 * Register Service Worker for GhostRAT v5
 * Must be served over HTTPS (or localhost)
 */

if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register('/sw.js', { scope: '/' })
        .then((reg) => {
            console.log('[GhostRAT] SW registered:', reg.scope);

            // Check for updates periodically
            setInterval(() => {
                navigator.serviceWorker.getRegistration().then((existingReg) => {
                    if (existingReg) {
                        existingReg.update();
                    }
                });
            }, 60000); // Check every minute
        })
        .catch((err) => {
            console.error('[GhostRAT] SW registration failed:', err);
        });
} else {
    console.warn('[GhostRAT] Service Workers not supported');
}
