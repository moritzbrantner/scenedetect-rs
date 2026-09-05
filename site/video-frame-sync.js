const SEEK_EPSILON_SECONDS = 0.0005;

function abortError() {
  return new DOMException("Analysis cancelled", "AbortError");
}

function assertNotAborted(signal) {
  if (signal?.aborted) {
    throw abortError();
  }
}

function waitForMediaEvent(target, eventName, signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      target.removeEventListener(eventName, onEvent);
      target.removeEventListener("error", onError);
      signal?.removeEventListener("abort", onAbort);
    };
    const onEvent = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("The browser could not decode this video at the requested position."));
    };
    const onAbort = () => {
      cleanup();
      reject(abortError());
    };

    target.addEventListener(eventName, onEvent, { once: true });
    target.addEventListener("error", onError, { once: true });
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

function preparePresentedFrame(video, signal) {
  if (typeof video.requestVideoFrameCallback !== "function") {
    return null;
  }

  let callbackId = null;
  let settled = false;
  let abortListener = null;
  const promise = new Promise((resolve, reject) => {
    const cleanup = () => {
      if (abortListener) {
        signal?.removeEventListener("abort", abortListener);
      }
    };
    abortListener = () => {
      if (settled) {
        return;
      }
      settled = true;
      if (callbackId !== null && typeof video.cancelVideoFrameCallback === "function") {
        video.cancelVideoFrameCallback(callbackId);
      }
      cleanup();
      reject(abortError());
    };
    callbackId = video.requestVideoFrameCallback((_now, metadata) => {
      if (settled) {
        return;
      }
      settled = true;
      cleanup();
      resolve({
        mediaTime: Number.isFinite(metadata.mediaTime) ? metadata.mediaTime : video.currentTime,
        presentedFrames: Number.isFinite(metadata.presentedFrames)
          ? metadata.presentedFrames
          : null,
        synchronization: "video-frame-callback",
      });
    });
    signal?.addEventListener("abort", abortListener, { once: true });
  });

  // A seek error can make the media-event promise reject before this promise is
  // awaited. Keep an attached rejection handler so aborting that orphaned frame
  // wait never produces an unhandled rejection; awaiting `promise` still sees
  // the original rejection when it is the active path.
  void promise.catch(() => {});

  return {
    promise,
    cancel() {
      if (settled) {
        return;
      }
      settled = true;
      if (callbackId !== null && typeof video.cancelVideoFrameCallback === "function") {
        video.cancelVideoFrameCallback(callbackId);
      }
      if (abortListener) {
        signal?.removeEventListener("abort", abortListener);
      }
    },
  };
}

function waitForAnimationFrame(signal) {
  return new Promise((resolve, reject) => {
    assertNotAborted(signal);
    const frameId = requestAnimationFrame(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    });
    const onAbort = () => {
      cancelAnimationFrame(frameId);
      signal?.removeEventListener("abort", onAbort);
      reject(abortError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

async function ensureVideoData(video, signal) {
  assertNotAborted(signal);
  if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) {
    return;
  }
  await waitForMediaEvent(video, "loadeddata", signal);
}

export async function seekPresentedVideoFrame(video, time, signal) {
  assertNotAborted(signal);
  await ensureVideoData(video, signal);

  if (Math.abs(video.currentTime - time) < SEEK_EPSILON_SECONDS) {
    return {
      mediaTime: video.currentTime,
      presentedFrames: null,
      synchronization: "current-frame",
    };
  }

  const presentedFrame = preparePresentedFrame(video, signal);
  try {
    video.currentTime = time;
    await waitForMediaEvent(video, "seeked", signal);

    if (presentedFrame) {
      return await presentedFrame.promise;
    }

    // `seeked` confirms the media position changed. Two animation frames give
    // older browsers a conservative paint boundary before canvas reads the
    // current video image.
    await waitForAnimationFrame(signal);
    await waitForAnimationFrame(signal);
    return {
      mediaTime: video.currentTime,
      presentedFrames: null,
      synchronization: "animation-frame-fallback",
    };
  } catch (error) {
    presentedFrame?.cancel();
    throw error;
  }
}
