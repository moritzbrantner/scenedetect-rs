import { createSceneDetect } from "./scenedetect-wasm.js";

const sceneDetectPromise = createSceneDetect();
let session = null;
let queue = Promise.resolve();

async function handleMessage(event) {
  const { id, type, payload = {} } = event.data ?? {};
  try {
    const sceneDetect = await sceneDetectPromise;
    let value = null;

    if (type === "ready") {
      value = { abi: sceneDetect.abiVersion };
    } else if (type === "defaults") {
      value = sceneDetect.defaults(payload.detector);
    } else if (type === "start") {
      session?.drop();
      session = sceneDetect.createSession(payload.config, payload.frameRate);
    } else if (type === "frame") {
      if (!session) {
        throw new Error("SceneDetect worker received a frame without an active session.");
      }
      session.pushFrame(
        payload.index,
        payload.width,
        payload.height,
        new Uint8Array(payload.rgb),
        payload.mediaTimeSeconds,
      );
    } else if (type === "finish") {
      if (!session) {
        throw new Error("SceneDetect worker has no active session to finish.");
      }
      const active = session;
      session = null;
      value = active.finish();
    } else if (type === "drop") {
      session?.drop();
      session = null;
    } else {
      throw new Error(`Unsupported SceneDetect worker message: ${type}`);
    }

    self.postMessage({ id, ok: true, value });
  } catch (error) {
    try {
      session?.drop();
    } catch (_dropError) {
      // The WASM session may already have been consumed while reporting the error.
    }
    session = null;
    self.postMessage({
      id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

self.addEventListener("message", (event) => {
  queue = queue.then(
    () => handleMessage(event),
    () => handleMessage(event),
  );
});
