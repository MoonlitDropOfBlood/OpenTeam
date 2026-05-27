import { createTransport } from './transport.js';
import { registerHook, triggerHook, getRegisteredHooks } from './registry.js';

const transport = createTransport();
console.error('Plugin Host started');

transport.onRequest(async (req) => {
  switch (req.method) {
    case 'ping':
      transport.sendResponse(req.id, { pong: true });
      break;

    case 'trigger_hook':
      const results = triggerHook(req.params.hook_point, req.params.payload);
      transport.sendResponse(req.id, { results });
      break;

    case 'get_hooks':
      transport.sendResponse(req.id, { hooks: getRegisteredHooks() });
      break;

    case 'load_plugin': {
      const { path } = req.params;
      try {
        const plugin = await import(path);
        if (plugin.default && plugin.default.setup) {
          plugin.default.setup({ registerHook });
        } else if (plugin.setup) {
          plugin.setup({ registerHook });
        }
        transport.sendResponse(req.id, { loaded: true, name: path });
      } catch (e) {
        transport.sendResponse(req.id, null, {
          code: -1,
          message: e.message,
        });
      }
      break;
    }

    default:
      transport.sendResponse(req.id, null, {
        code: -32601,
        message: `Method not found: ${req.method}`,
      });
  }
});
