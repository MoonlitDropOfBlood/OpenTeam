import readline from 'readline';

export function createTransport() {
  const rl = readline.createInterface({ input: process.stdin });
  let requestId = 0;

  function sendResponse(id, result, error) {
    const msg = JSON.stringify({ jsonrpc: '2.0', id, result, error });
    process.stdout.write(msg + '\n');
  }

  function sendRequest(method, params) {
    requestId++;
    const msg = JSON.stringify({ jsonrpc: '2.0', id: requestId, method, params });
    process.stdout.write(msg + '\n');
    return new Promise((resolve) => {
      rl.once('line', (line) => {
        try { resolve(JSON.parse(line)); }
        catch { resolve(null); }
      });
    });
  }

  function onRequest(handler) {
    rl.on('line', (line) => {
      try {
        const req = JSON.parse(line);
        handler(req);
      } catch (e) {
        console.error('Invalid JSON-RPC:', line);
      }
    });
  }

  return { sendResponse, sendRequest, onRequest };
}
