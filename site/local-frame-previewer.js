import { seekPresentedVideoFrame } from "./video-frame-sync.js";

function waitForMetadata(video, signal) {
  if (video.readyState >= HTMLMediaElement.HAVE_METADATA && Number.isFinite(video.duration)) {
    return Promise.resolve();
  }
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      video.removeEventListener("loadedmetadata", onLoaded);
      video.removeEventListener("error", onError);
      signal?.removeEventListener("abort", onAbort);
    };
    const onLoaded = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("The browser could not decode the selected local preview."));
    };
    const onAbort = () => {
      cleanup();
      reject(new DOMException("Local frame preview cancelled", "AbortError"));
    };
    video.addEventListener("loadedmetadata", onLoaded, { once: true });
    video.addEventListener("error", onError, { once: true });
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

function throwIfAborted(signal) {
  if (signal?.aborted) {
    throw new DOMException("Local frame preview cancelled", "AbortError");
  }
}

function positiveInteger(value, fallback) {
  const numeric = Math.floor(Number(value));
  return Number.isFinite(numeric) && numeric > 0 ? numeric : fallback;
}

export function evenlySpacedPreviewTimes(start, end, count = 3) {
  const first = Number(start);
  const last = Number(end);
  if (!Number.isFinite(first) || !Number.isFinite(last)) {
    return [];
  }
  const lower = Math.min(first, last);
  const upper = Math.max(first, last);
  const total = positiveInteger(count, 1);
  if (total === 1 || upper - lower <= 1e-6) {
    return [(lower + upper) / 2];
  }
  return Array.from({ length: total }, (_value, index) =>
    lower + ((upper - lower) * index) / (total - 1),
  );
}

export function createLocalFramePreviewer(
  sourceVideo,
  {
    cacheLimit = 96,
    maxWidth = 640,
    maxHeight = 360,
    quality = 0.86,
  } = {},
) {
  const previewVideo = document.createElement("video");
  previewVideo.muted = true;
  previewVideo.playsInline = true;
  previewVideo.preload = "auto";
  previewVideo.setAttribute("aria-hidden", "true");
  Object.assign(previewVideo.style, {
    height: "2px",
    left: "-10000px",
    opacity: "0",
    pointerEvents: "none",
    position: "fixed",
    top: "0",
    width: "2px",
  });
  document.body.append(previewVideo);

  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  const cache = new Map();
  const limit = positiveInteger(cacheLimit, 96);
  let previewSource = "";
  let queue = Promise.resolve();

  function remember(key, dataUrl) {
    cache.delete(key);
    cache.set(key, dataUrl);
    while (cache.size > limit) {
      cache.delete(cache.keys().next().value);
    }
  }

  function serialized(task) {
    const result = queue.then(task, task);
    queue = result.catch(() => undefined);
    return result;
  }

  async function ensurePreviewVideo(signal) {
    throwIfAborted(signal);
    const source = sourceVideo.currentSrc || sourceVideo.src;
    if (!source) {
      throw new Error("No local video is available for frame previews.");
    }
    if (source !== previewSource) {
      previewSource = source;
      cache.clear();
      previewVideo.src = source;
      previewVideo.load();
    }
    await waitForMetadata(previewVideo, signal);
    throwIfAborted(signal);
  }

  async function capture(mediaTime, options = {}) {
    return serialized(async () => {
      const signal = options.signal;
      await ensurePreviewVideo(signal);
      const widthLimit = positiveInteger(options.maxWidth, positiveInteger(maxWidth, 640));
      const heightLimit = positiveInteger(options.maxHeight, positiveInteger(maxHeight, 360));
      const jpegQuality = Number.isFinite(Number(options.quality))
        ? Math.min(1, Math.max(0.1, Number(options.quality)))
        : quality;
      const time = Math.min(
        Math.max(0, previewVideo.duration - 0.001),
        Math.max(0, Number(mediaTime) || 0),
      );
      const key = `${previewSource}|${time.toFixed(6)}|${widthLimit}x${heightLimit}|${jpegQuality.toFixed(3)}`;
      const cached = cache.get(key);
      if (cached) {
        remember(key, cached);
        return cached;
      }

      await seekPresentedVideoFrame(previewVideo, time, signal);
      throwIfAborted(signal);
      if (!context || !previewVideo.videoWidth || !previewVideo.videoHeight) {
        throw new Error("The selected local frame could not be drawn.");
      }
      const scale = Math.min(
        1,
        widthLimit / previewVideo.videoWidth,
        heightLimit / previewVideo.videoHeight,
      );
      canvas.width = Math.max(1, Math.round(previewVideo.videoWidth * scale));
      canvas.height = Math.max(1, Math.round(previewVideo.videoHeight * scale));
      context.drawImage(previewVideo, 0, 0, canvas.width, canvas.height);
      const dataUrl = canvas.toDataURL("image/jpeg", jpegQuality);
      remember(key, dataUrl);
      return dataUrl;
    });
  }

  async function captureMany(times, options = {}) {
    const previews = [];
    for (const time of times) {
      throwIfAborted(options.signal);
      previews.push({
        time,
        url: await capture(time, options),
      });
    }
    return previews;
  }

  function reset() {
    cache.clear();
    previewSource = "";
    previewVideo.removeAttribute("src");
    previewVideo.load();
  }

  function dispose() {
    reset();
    previewVideo.remove();
  }

  return {
    capture,
    captureMany,
    reset,
    dispose,
  };
}
