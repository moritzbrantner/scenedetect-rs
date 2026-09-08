const SETTINGS_KEY = "scenedetect-rs.workbench.settings.v1";
const RUNS_KEY = "scenedetect-rs.workbench.runs.v1";

function encodeJson(value) {
  const bytes = new TextEncoder().encode(JSON.stringify(value));
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "");
}

function decodeJson(value) {
  const normalized = value.replaceAll("-", "+").replaceAll("_", "/");
  const padded = normalized + "=".repeat((4 - (normalized.length % 4 || 4)) % 4);
  const binary = atob(padded);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return JSON.parse(new TextDecoder().decode(bytes));
}

function safeParse(value, fallback) {
  try {
    return JSON.parse(value);
  } catch (_error) {
    return fallback;
  }
}

export function loadWorkbenchSettings() {
  const url = new URL(window.location.href);
  const encoded = url.searchParams.get("config");
  if (encoded) {
    try {
      return decodeJson(encoded);
    } catch (_error) {
      // Fall back to local settings if an old or edited URL is malformed.
    }
  }
  return safeParse(localStorage.getItem(SETTINGS_KEY), null);
}

export function saveWorkbenchSettings(settings) {
  localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  const url = new URL(window.location.href);
  url.searchParams.set("config", encodeJson(settings));
  history.replaceState(null, "", url);
}

export function listRunSnapshots() {
  const snapshots = safeParse(localStorage.getItem(RUNS_KEY), []);
  return Array.isArray(snapshots) ? snapshots : [];
}

export function saveRunSnapshot(snapshot) {
  const snapshots = listRunSnapshots().filter((entry) => entry.id !== snapshot.id);
  snapshots.unshift(snapshot);
  const bounded = snapshots.slice(0, 8);
  localStorage.setItem(RUNS_KEY, JSON.stringify(bounded));
  return bounded;
}

export function mediaFingerprint(file) {
  if (!file) {
    return null;
  }
  return {
    name: file.name,
    size: file.size,
    last_modified: file.lastModified,
  };
}

export function fingerprintsMatch(left, right) {
  return Boolean(
    left &&
      right &&
      left.size === right.size &&
      left.last_modified === right.last_modified,
  );
}
