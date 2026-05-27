const hooks = new Map();

export function registerHook(hookPoint, handler) {
  if (!hooks.has(hookPoint)) {
    hooks.set(hookPoint, []);
  }
  hooks.get(hookPoint).push(handler);
}

export function triggerHook(hookPoint, payload) {
  const handlers = hooks.get(hookPoint) || [];
  return handlers.map(fn => fn(payload));
}

export function getRegisteredHooks() {
  const result = {};
  for (const [point, handlers] of hooks) {
    result[point] = handlers.length;
  }
  return result;
}
