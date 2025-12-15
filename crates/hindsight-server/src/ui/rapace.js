// Rapace RPC Client for Browser (over WebSocket)
// Pure JavaScript implementation of Rapace binary protocol

class RapaceClient {
    constructor(url) {
        this.url = url;
        this.ws = null;
        this.nextChannelId = 1;
        this.pendingCalls = new Map(); // channelId -> { resolve, reject }
    }

    async connect() {
        return new Promise((resolve, reject) => {
            this.ws = new WebSocket(this.url);
            this.ws.binaryType = 'arraybuffer';

            this.ws.onopen = () => {
                console.log('🔌 Rapace WebSocket connected');
                resolve();
            };

            this.ws.onerror = (error) => {
                console.error('❌ Rapace WebSocket error:', error);
                reject(error);
            };

            this.ws.onmessage = (event) => {
                this.handleMessage(event.data);
            };

            this.ws.onclose = () => {
                console.log('🔌 Rapace WebSocket closed');
                // Reject all pending calls
                for (const [channelId, { reject }] of this.pendingCalls) {
                    reject(new Error('WebSocket closed'));
                }
                this.pendingCalls.clear();
            };
        });
    }

    handleMessage(data) {
        const view = new DataView(data);

        // Parse descriptor (8 bytes little-endian)
        const channelId = view.getUint32(0, true);  // little-endian
        const methodId = view.getUint32(4, true);

        // Extract payload
        const payload = new Uint8Array(data, 8);

        console.log(`📨 Rapace response: channel=${channelId}, method=${methodId}, payload_len=${payload.length}`);

        // Find pending call
        const pending = this.pendingCalls.get(channelId);
        if (pending) {
            this.pendingCalls.delete(channelId);

            try {
                // Decode Facet payload (for now, we'll use a simple decoder)
                const result = this.decodeFacet(payload);
                pending.resolve(result);
            } catch (error) {
                pending.reject(error);
            }
        } else {
            console.warn(`No pending call for channel ${channelId}`);
        }
    }

    async call(methodId, args) {
        const channelId = this.nextChannelId++;

        // Encode arguments as Facet (simplified for now)
        const payload = this.encodeFacet(args);

        // Build Rapace frame: [descriptor][payload]
        const frame = new ArrayBuffer(8 + payload.byteLength);
        const view = new DataView(frame);

        view.setUint32(0, channelId, true);  // little-endian
        view.setUint32(4, methodId, true);

        new Uint8Array(frame, 8).set(new Uint8Array(payload));

        console.log(`📤 Rapace call: channel=${channelId}, method=${methodId}, payload_len=${payload.byteLength}`);

        // Send frame
        this.ws.send(frame);

        // Return promise that resolves when response arrives
        return new Promise((resolve, reject) => {
            this.pendingCalls.set(channelId, { resolve, reject });

            // Timeout after 30 seconds
            setTimeout(() => {
                if (this.pendingCalls.has(channelId)) {
                    this.pendingCalls.delete(channelId);
                    reject(new Error('RPC call timeout'));
                }
            }, 30000);
        });
    }

    // Simplified Facet encoder (just for our use case)
    encodeFacet(value) {
        // For now, we'll use JSON as a temporary hack
        // TODO: Implement proper Facet encoding
        const json = JSON.stringify(value);
        return new TextEncoder().encode(json);
    }

    // Simplified Facet decoder
    decodeFacet(bytes) {
        // For now, we'll use JSON as a temporary hack
        // TODO: Implement proper Facet decoding
        const json = new TextDecoder().decode(bytes);
        return JSON.parse(json);
    }

    close() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}

// HindsightService client
class HindsightServiceClient {
    constructor(rapaceClient) {
        this.client = rapaceClient;

        // Method IDs for HindsightService
        // These need to match the server's method IDs
        // TODO: Get these from service introspection
        this.METHOD_LIST_TRACES = 2;
        this.METHOD_GET_TRACE = 3;
    }

    async listTraces(filter = {}) {
        return await this.client.call(this.METHOD_LIST_TRACES, filter);
    }

    async getTrace(traceId) {
        return await this.client.call(this.METHOD_GET_TRACE, { trace_id: traceId });
    }
}

// Export for use in app.js
window.RapaceClient = RapaceClient;
window.HindsightServiceClient = HindsightServiceClient;
