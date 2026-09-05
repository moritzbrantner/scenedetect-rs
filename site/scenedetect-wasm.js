const encoder = new TextEncoder();
const decoder = new TextDecoder();

async function instantiateModule() {
  const url = new URL("./wasm/scenedetect_wasm.wasm", import.meta.url);
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Unable to load SceneDetect WASM (HTTP ${response.status}).`);
  }

  if (WebAssembly.instantiateStreaming) {
    try {
      return await WebAssembly.instantiateStreaming(response.clone(), {});
    } catch (_error) {
      // Some static hosts serve .wasm with a generic MIME type. Fall back to
      // ArrayBuffer instantiation so the workbench still works there.
    }
  }
  return WebAssembly.instantiate(await response.arrayBuffer(), {});
}

export async function createSceneDetect() {
  const { instance } = await instantiateModule();
  const wasm = instance.exports;

  if (wasm.scenedetect_abi_version() !== 1) {
    throw new Error("Unsupported SceneDetect WASM ABI version.");
  }
  if (!wasm.memory) {
    throw new Error("SceneDetect WASM did not export linear memory.");
  }

  function readBytes(ptr, len) {
    if (!len) {
      return new Uint8Array();
    }
    return new Uint8Array(wasm.memory.buffer, ptr, len).slice();
  }

  function readResultText() {
    return decoder.decode(
      readBytes(wasm.scenedetect_result_ptr(), wasm.scenedetect_result_len()),
    );
  }

  function readError() {
    const text = decoder.decode(
      readBytes(wasm.scenedetect_error_ptr(), wasm.scenedetect_error_len()),
    );
    return text || "SceneDetect WASM operation failed.";
  }

  function check(code) {
    if (code !== 0) {
      throw new Error(readError());
    }
  }

  function withBytes(bytes, operation) {
    if (bytes.length === 0) {
      return operation(0, 0);
    }
    const ptr = wasm.scenedetect_alloc(bytes.length);
    if (!ptr) {
      throw new Error("SceneDetect WASM could not allocate input memory.");
    }
    try {
      new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
      return operation(ptr, bytes.length);
    } finally {
      wasm.scenedetect_dealloc(ptr, bytes.length);
    }
  }

  function defaults(detector) {
    const code = withBytes(encoder.encode(detector), (ptr, len) =>
      wasm.scenedetect_defaults(ptr, len),
    );
    check(code);
    return JSON.parse(readResultText());
  }

  function createSession(config, frameRate) {
    const configBytes = encoder.encode(JSON.stringify(config));
    const handle = withBytes(configBytes, (ptr, len) =>
      wasm.scenedetect_session_new(ptr, len, frameRate),
    );
    if (!handle) {
      throw new Error(readError());
    }

    let live = true;
    return {
      pushFrame(index, width, height, rgb) {
        if (!live) {
          throw new Error("SceneDetect session is already finished.");
        }
        const code = withBytes(rgb, (ptr, len) =>
          wasm.scenedetect_session_push(handle, index, width, height, ptr, len),
        );
        check(code);
      },
      finish() {
        if (!live) {
          throw new Error("SceneDetect session is already finished.");
        }
        live = false;
        const code = wasm.scenedetect_session_finish(handle);
        check(code);
        return JSON.parse(readResultText());
      },
      drop() {
        if (!live) {
          return;
        }
        live = false;
        const code = wasm.scenedetect_session_drop(handle);
        check(code);
      },
    };
  }

  return { defaults, createSession };
}
