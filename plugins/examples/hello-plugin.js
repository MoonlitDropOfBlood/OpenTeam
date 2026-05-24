import { createPlugin } from '../host/sdk/index.js';

export default createPlugin({
  name: 'hello',
  hooks: {
    'system:startup': (payload) => {
      console.error('[hello] System started!');
    },
    'message:received': (payload) => {
      if (payload && payload.content) {
        console.error(`[hello] Message: ${payload.content.substring(0, 40)}`);
      }
    },
  },
});
