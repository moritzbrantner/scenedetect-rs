export function createAnalysisWorker() {
  const worker = new Worker(new URL("./analysis-worker.js", import.meta.url), { type: "module" });
  const pending = new Map();
  let requestId = 0;
  let liveSession = false;
  let closed = false;

  worker.addEventListener("message", (event) => {
    const { id, ok, value, error } = event.data ?? {};
    const entry = pending.get(id);
    if (!entry) {
      return;
    }
    pending.delete(id);
    if (ok) {
      entry.resolve(value);
    } else {
      entry.reject(new Error(error || "SceneDetect worker operation failed."));
    }
  });

  worker.addEventListener("error", (event) => {
    const error = event.error ?? new Error(event.message || "SceneDetect worker failed.");
    for (const entry of pending.values()) {
      entry.reject(error);
    }
    pending.clear();
  });

  function request(type, payload = {}, transfer = []) {
    if (closed) {
      return Promise.reject(new Error("SceneDetect worker is closed."));
    }
    const id = ++requestId;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      worker.postMessage({ id, type, payload }, transfer);
    });
  }

  return {
    ready() {
      return request("ready");
    },

    defaults(detector) {
      return request("defaults", { detector });
    },

    async start(config, frameRate) {
      if (liveSession) {
        await this.drop();
      }
      await request("start", { config, frameRate });
      liveSession = true;
    },

    pushFrame(index, width, height, mediaTimeSeconds, rgb) {
      if (!liveSession) {
        return Promise.reject(new Error("SceneDetect worker session is not active."));
      }
      const bytes =
        rgb.byteOffset === 0 && rgb.byteLength === rgb.buffer.byteLength
          ? rgb
          : rgb.slice();
      return request(
        "frame",
        {
          index,
          width,
          height,
          mediaTimeSeconds,
          rgb: bytes.buffer,
        },
        [bytes.buffer],
      );
    },

    async finish() {
      if (!liveSession) {
        throw new Error("SceneDetect worker session is not active.");
      }
      liveSession = false;
      return request("finish");
    },

    async drop() {
      if (!liveSession) {
        return;
      }
      liveSession = false;
      await request("drop");
    },

    close() {
      closed = true;
      liveSession = false;
      worker.terminate();
      for (const entry of pending.values()) {
        entry.reject(new Error("SceneDetect worker was closed."));
      }
      pending.clear();
    },
  };
}
