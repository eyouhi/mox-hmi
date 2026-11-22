(() => {
  const prototype = globalThis.GPUAdapterInfo?.prototype;
  if (!prototype) return;

  // wgpu 30 reads both fields unconditionally. Current WebKit exposes
  // GPUAdapterInfo without them, so use the WebGPU spec's guaranteed bounds.
  for (const [name, fallback] of [
    ["subgroupMinSize", 4],
    ["subgroupMaxSize", 128],
  ]) {
    if (name in prototype) continue;
    Object.defineProperty(prototype, name, {
      configurable: true,
      get() {
        return fallback;
      },
    });
  }
})();
