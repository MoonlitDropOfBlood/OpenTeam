export function createPlugin(options) {
  return {
    name: options.name,
    hooks: options.hooks || {},
    setup(ctx) {
      for (const [hookPoint, handler] of Object.entries(this.hooks)) {
        ctx.registerHook(hookPoint, handler);
      }
      console.error(`[Plugin ${this.name}] initialized`);
    },
  };
}
