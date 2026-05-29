#!/usr/bin/env node

/**
 * Feishu Channel Plugin
 *
 * Bridges the Feishu Channel SDK (`@larksuiteoapi/node-sdk`) with the Rust core
 * via JSON-RPC 2.0 over stdio.
 *
 * Protocol:
 *   stdin  ← JSON-RPC requests from Rust (one JSON object per line)
 *   stdout → JSON-RPC responses and notifications to Rust (one JSON object per line)
 *   stderr → logging (consumed by Rust as tracing output)
 *
 * Request/Response (Rust → Plugin → Rust):
 *   {"jsonrpc":"2.0","id":1,"method":"connect","params":{...}}
 *   {"jsonrpc":"2.0","id":1,"result":{...}}
 *
 * Notification (Plugin → Rust, unsolicited):
 *   {"jsonrpc":"2.0","id":null,"method":"feishu:message","params":{...}}
 */

const { createLarkChannel } = require('@larksuiteoapi/node-sdk');
const readline = require('readline');

// ---------- State ----------

let channel = null;
let connected = false;

// ---------- JSON-RPC helpers ----------

/**
 * Write a JSON-RPC response to stdout.
 * @param {number|null} id
 * @param {*} result
 * @param {*} [error]
 */
function writeResponse(id, result, error) {
    const msg = { jsonrpc: '2.0', id };
    if (error) {
        msg.error = { code: error.code || -1, message: error.message || String(error) };
    } else {
        msg.result = result;
    }
    process.stdout.write(JSON.stringify(msg) + '\n');
}

/**
 * Write a JSON-RPC notification to stdout (id is always null).
 * Notifications are events pushed from the plugin to Rust unsolicited.
 * @param {string} method  e.g. "feishu:message", "feishu:ready"
 * @param {*} params
 */
function writeNotification(method, params) {
    const msg = {
        jsonrpc: '2.0',
        id: null,
        method,
        params: params || {},
    };
    process.stdout.write(JSON.stringify(msg) + '\n');
}

// ---------- Channel SDK wrapper ----------

function getSendContent(input) {
    // input can be: { text }, { markdown }, { card }, { image }, { file }, etc.
    return input;
}

/**
 * Handle an incoming JSON-RPC request.
 */
async function handleRequest(request) {
    const { id, method, params } = request;

    try {
        switch (method) {
            // ---- Lifecycle ----
            case 'connect': {
                const { appId, appSecret, domain } = params || {};
                if (!appId || !appSecret) {
                    writeResponse(id, null, { code: -32602, message: 'appId and appSecret are required' });
                    return;
                }

                // Create the channel with Feishu Channel SDK
                channel = createLarkChannel({
                    appId,
                    appSecret,
                    domain: domain || undefined,
                    policy: {
                        requireMention: true,
                        dmMode: 'open',
                    },
                });

                // Register event listeners BEFORE connecting
                channel.on('message', (msg) => {
                    writeNotification('feishu:message', msg);
                });

                channel.on('cardAction', (evt) => {
                    writeNotification('feishu:card_action', evt);
                });

                channel.on('reaction', (evt) => {
                    writeNotification('feishu:reaction', evt);
                });

                channel.on('botAdded', (evt) => {
                    writeNotification('feishu:bot_added', evt);
                });

                channel.on('reconnecting', () => {
                    connected = false;
                    writeNotification('feishu:reconnecting', {});
                });

                channel.on('reconnected', () => {
                    connected = true;
                    writeNotification('feishu:reconnected', {});
                });

                channel.on('error', (err) => {
                    writeNotification('feishu:error', {
                        message: err?.message || String(err),
                    });
                });

                // Connect (handshake with 15s timeout)
                await channel.connect();
                connected = true;

                writeResponse(id, {
                    connected: true,
                    botName: channel.botIdentity?.name || '',
                    botOpenId: channel.botIdentity?.openId || '',
                });

                // Notify Rust that we're ready
                writeNotification('feishu:ready', {
                    botName: channel.botIdentity?.name || '',
                });

                break;
            }

            // ---- Disconnect ----
            case 'disconnect': {
                if (channel) {
                    await channel.disconnect();
                }
                connected = false;
                channel = null;
                writeResponse(id, { success: true });
                break;
            }

            // ---- Send message ----
            case 'send': {
                if (!channel || !connected) {
                    writeResponse(id, null, { code: -32000, message: 'not connected' });
                    return;
                }
                const { chatId, content, options } = params || {};
                if (!chatId || !content) {
                    writeResponse(id, null, { code: -32602, message: 'chatId and content are required' });
                    return;
                }

                const response = await channel.send(chatId, getSendContent(content), options || {});
                writeResponse(id, { messageId: response?.messageId || response || '' });
                break;
            }

            // ---- Stream reply ----
            case 'stream': {
                if (!channel || !connected) {
                    writeResponse(id, null, { code: -32000, message: 'not connected' });
                    return;
                }
                const { chatId, chunks, content, options } = params || {};

                // Support both pre-collected chunks and a single content value
                if (chunks && Array.isArray(chunks)) {
                    // Pre-collected chunks: simulate streaming
                    await channel.stream(chatId, {
                        markdown: async (s) => {
                            for (const chunk of chunks) {
                                await s.append(chunk);
                            }
                        },
                    }, options || {});
                } else if (content) {
                    // Single content: send as one chunk
                    await channel.stream(chatId, {
                        markdown: async (s) => {
                            await s.append(content);
                        },
                    }, options || {});
                } else {
                    writeResponse(id, null, { code: -32602, message: 'chunks[] or content required' });
                    return;
                }

                writeResponse(id, { success: true });
                break;
            }

            // ---- Policy update ----
            case 'updatePolicy': {
                if (!channel) {
                    writeResponse(id, null, { code: -32000, message: 'channel not initialized' });
                    return;
                }
                channel.updatePolicy(params);
                writeResponse(id, { success: true });
                break;
            }

            // ---- Bot info ----
            case 'getBotInfo': {
                if (!channel || !connected) {
                    writeResponse(id, null, { code: -32000, message: 'not connected' });
                    return;
                }
                const info = channel.botIdentity || {};
                writeResponse(id, {
                    botName: info.name || '',
                    botOpenId: info.openId || '',
                    activateStatus: info.activateStatus || '',
                });
                break;
            }

            // ---- Connection status ----
            case 'getConnectionStatus': {
                writeResponse(id, {
                    connected,
                    botName: channel?.botIdentity?.name || null,
                });
                break;
            }

            // ---- Ping (health check) ----
            case 'ping': {
                writeResponse(id, { pong: true });
                break;
            }

            default: {
                writeResponse(id, null, {
                    code: -32601,
                    message: `Method not found: ${method}`,
                });
            }
        }
    } catch (err) {
        writeResponse(id, null, {
            code: -1,
            message: err?.message || String(err),
        });
    }
}

// ---------- Main ----------

function main() {
    // Log diagnostics to stderr (captured by Rust as tracing output)
    console.error = (...args) => {
        process.stderr.write('[feishu-channel] ' + args.join(' ') + '\n');
    };

    // No startup message — stderr is piped to tracing by the Rust host

    const rl = readline.createInterface({
        input: process.stdin,
        output: process.stdout,  // stdout is used for JSON-RPC responses, not console.log
        terminal: false,
    });

    rl.on('line', (line) => {
        line = line.trim();
        if (!line) return;

        let request;
        try {
            request = JSON.parse(line);
        } catch (e) {
            writeResponse(null, null, { code: -32700, message: 'Parse error: ' + e.message });
            return;
        }

        // Validate JSON-RPC request
        if (request.jsonrpc !== '2.0' || !request.method) {
            writeResponse(request?.id ?? null, null, { code: -32600, message: 'Invalid Request' });
            return;
        }

        // Fire-and-forget if no id (notification from Rust, not expected but handle gracefully)
        if (request.id === null || request.id === undefined) {
            return;
        }

        handleRequest(request).catch((err) => {
            writeResponse(request.id, null, { code: -1, message: err?.message || String(err) });
        });
    });

    rl.on('close', () => {
        console.error('stdin closed, shutting down');
        if (channel) {
            channel.disconnect().catch(() => {});
        }
        process.exit(0);
    });

    process.on('SIGINT', () => {
        console.error('SIGINT received');
        rl.close();
    });

    process.on('SIGTERM', () => {
        console.error('SIGTERM received');
        rl.close();
    });
}

main();