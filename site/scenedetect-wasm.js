const encoder = new TextEncoder();
const decoder = new TextDecoder();
const SUPPORTED_ABI_VERSION = 2;

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
  const abiVersion = wasm.scenedetect_abi_version();

  if (abiVersion !== SUPPORTED_ABI_VERSION) {
    throw new Error(`Unsupported SceneDetect WASM ABI version ${abiVersion}.`);
  }
  if (!wasm.memory) {
    throw new Error("SceneDetect WASM did not export linear memory.");
  }
  for (const name of [
    "scenedetect_similarity_reset",
    "scenedetect_similarity_push",
    "scenedetect_similarity_finish",
    "scenedetect_similarity_result_ptr",
    "scenedetect_similarity_result_len",
    "scenedetect_similarity_error_ptr",
    "scenedetect_similarity_error_len",
  ]) {
    if (typeof wasm[name] !== "function") {
      throw new Error(`SceneDetect WASM is missing required browser analysis export ${name}.`);
    }
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

  function readSimilarityResultText() {
    return decoder.decode(
      readBytes(
        wasm.scenedetect_similarity_result_ptr(),
        wasm.scenedetect_similarity_result_len(),
      ),
    );
  }

  function readSimilarityError() {
    const text = decoder.decode(
      readBytes(
        wasm.scenedetect_similarity_error_ptr(),
        wasm.scenedetect_similarity_error_len(),
      ),
    );
    return text || "SceneDetect similarity operation failed.";
  }

  function check(code) {
    if (code !== 0) {
      throw new Error(readError());
    }
  }

  function checkSimilarity(code) {
    if (code !== 0) {
      throw new Error(readSimilarityError());
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
    checkSimilarity(wasm.scenedetect_similarity_reset());
    const configBytes = encoder.encode(JSON.stringify(config));
    const handle = withBytes(configBytes, (ptr, len) =>
      wasm.scenedetect_session_new(ptr, len, frameRate),
    );
    if (!handle) {
      throw new Error(readError());
    }

    let live = true;
    return {
      pushFrame(index, width, height, rgb, mediaTimeSeconds) {
        if (!live) {
          throw new Error("SceneDetect session is already finished.");
        }
        if (!Number.isFinite(mediaTimeSeconds) || mediaTimeSeconds < 0) {
          throw new Error("Presented media time must be a non-negative finite number.");
        }
        const code = withBytes(rgb, (ptr, len) => {
          checkSimilarity(wasm.scenedetect_similarity_push(index, width, height, ptr, len));
          return wasm.scenedetect_session_push(
            handle,
            index,
            width,
            height,
            mediaTimeSeconds,
            ptr,
            len,
          );
        });
        check(code);
      },
      finish() {
        if (!live) {
          throw new Error("SceneDetect session is already finished.");
        }
        live = false;
        const code = wasm.scenedetect_session_finish(handle);
        check(code);
        const output = JSON.parse(readResultText());
        const sceneListBytes = encoder.encode(JSON.stringify(output.detection.scene_list));
        const similarityCode = withBytes(sceneListBytes, (ptr, len) =>
          wasm.scenedetect_similarity_finish(ptr, len),
        );
        checkSimilarity(similarityCode);
        output.scene_similarity = JSON.parse(readSimilarityResultText());
        return output;
      },
      drop() {
        if (!live) {
          return;
        }
        live = false;
        checkSimilarity(wasm.scenedetect_similarity_reset());
        const code = wasm.scenedetect_session_drop(handle);
        check(code);
      },
    };
  }

  return { abiVersion, defaults, createSession };
}