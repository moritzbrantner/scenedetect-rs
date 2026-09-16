const STORAGE_KEY = "scenedetect-rs.workbench.keyboard.v1";

export const INPUT_BINDINGS_BUNDLE_URL =
  "https://moritzbrantner.github.io/input-bindings/input-bindings-browser.js";

export const DEFAULT_KEY_BINDINGS = Object.freeze({
  previous_boundary: "k",
  next_boundary: "j",
  previous_scene: "[",
  next_scene: "]",
  play_pause: "Space",
  accept_boundary: "a",
  reject_boundary: "x",
  add_cut: "c",
  merge_next: "m",
  zoom_in: "+",
  zoom_out: "-",
});

const LABELS = Object.freeze({
  previous_boundary: "Previous boundary",
  next_boundary: "Next boundary",
  previous_scene: "Previous scene",
  next_scene: "Next scene",
  play_pause: "Play / pause",
  accept_boundary: "Accept selected boundary",
  reject_boundary: "Reject selected boundary",
  add_cut: "Add cut at playhead",
  merge_next: "Merge selected scene with next",
  zoom_in: "Zoom timeline in",
  zoom_out: "Zoom timeline out",
});

const CONTEXT_ID = "timelineWorkbench";

function normalizedKeyValue(value) {
  if (value === " ") {
    return "Space";
  }
  return value.length === 1 ? value.toLocaleLowerCase() : value;
}

function keyStroke(value) {
  return {
    key: { kind: "logical", value: normalizedKeyValue(value) },
    modifiers: {},
  };
}

function bindingId(command) {
  return `scenedetect.workbench.${command}`;
}

const REGISTRY = Object.freeze({
  actions: Object.keys(DEFAULT_KEY_BINDINGS).map((command) => ({
    id: command,
    title: LABELS[command],
    categoryPath: ["Timeline workbench"],
    repeatPolicy: "never",
    allowedDevices: ["keyboard"],
    defaults: [
      {
        id: bindingId(command),
        action: command,
        sequence: [keyStroke(DEFAULT_KEY_BINDINGS[command])],
        when: { op: "context", id: CONTEXT_ID },
        priority: 0,
      },
    ],
    provenance: { source: "scenedetect-rs/workbench", version: "1" },
  })),
});

function loadBindings() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY));
    if (saved && typeof saved === "object") {
      return { ...DEFAULT_KEY_BINDINGS, ...saved };
    }
  } catch (_error) {
    // Ignore malformed local state and keep deterministic defaults.
  }
  return { ...DEFAULT_KEY_BINDINGS };
}

function saveBindings(bindings) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(bindings));
}

function profileFromBindings(bindings) {
  const patches = [];
  for (const command of Object.keys(DEFAULT_KEY_BINDINGS)) {
    const value = normalizedKeyValue(bindings[command]);
    const defaultValue = normalizedKeyValue(DEFAULT_KEY_BINDINGS[command]);
    if (value === defaultValue) {
      continue;
    }
    patches.push({
      op: "replace",
      bindingId: bindingId(command),
      binding: {
        id: bindingId(command),
        action: command,
        sequence: [keyStroke(value)],
        when: { op: "context", id: CONTEXT_ID },
        priority: 0,
      },
    });
  }
  return { id: "scenedetect-workbench-user", patches };
}

function normalizedKey(event) {
  if (event.key === " ") {
    return "Space";
  }
  if (event.key.length === 1) {
    return event.key.toLocaleLowerCase();
  }
  return event.key;
}

export function createKeyboardController({ container, actions }) {
  let bindings = loadBindings();
  let runtimeController = null;
  let detachRuntime = () => {};
  let destroyed = false;
  const fields = new Map();

  const updateRuntimeConfiguration = () => {
    runtimeController?.updateConfiguration(REGISTRY, profileFromBindings(bindings));
  };

  function render() {
    container.replaceChildren();
    const grid = document.createElement("div");
    grid.className = "shortcut-grid";

    for (const command of Object.keys(DEFAULT_KEY_BINDINGS)) {
      const label = document.createElement("label");
      label.className = "shortcut-field";
      const title = document.createElement("span");
      title.textContent = LABELS[command];
      const input = document.createElement("input");
      input.type = "text";
      input.readOnly = true;
      input.value = bindings[command];
      input.dataset.command = command;
      input.setAttribute("aria-label", `${LABELS[command]} shortcut`);
      input.addEventListener("keydown", (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === "Escape") {
          input.blur();
          return;
        }
        const key = normalizedKey(event);
        if (!key || key === "Dead" || key === "Process" || event.isComposing) {
          return;
        }
        bindings = { ...bindings, [command]: key };
        saveBindings(bindings);
        updateRuntimeConfiguration();
        input.value = key;
        input.blur();
      });
      label.append(title, input);
      grid.append(label);
      fields.set(command, input);
    }

    const reset = document.createElement("button");
    reset.type = "button";
    reset.className = "action-button";
    reset.textContent = "Reset shortcuts";
    reset.addEventListener("click", () => {
      bindings = { ...DEFAULT_KEY_BINDINGS };
      saveBindings(bindings);
      updateRuntimeConfiguration();
      for (const [command, input] of fields) {
        input.value = bindings[command];
      }
    });

    container.append(grid, reset);
  }

  render();

  const ready = import(INPUT_BINDINGS_BUNDLE_URL).then(
    ({ InputRuntimeController, attachKeyboardRuntime }) => {
      if (destroyed) {
        return;
      }
      runtimeController = new InputRuntimeController({
        registry: REGISTRY,
        profile: profileFromBindings(bindings),
        getActiveContexts: () => new Set([CONTEXT_ID]),
        consumePolicy: "dispatched",
        onDispatch: (dispatch) => {
          if (dispatch.phase !== "press") {
            return;
          }
          actions[dispatch.action]?.();
        },
      });
      detachRuntime = attachKeyboardRuntime(runtimeController, {
        keyTarget: document,
        focusTarget: window,
        visibilityTarget: document,
        ignoreTextEntry: true,
        mode: "logical",
      });
    },
    (error) => {
      console.error("Failed to load shared input-bindings runtime", error);
      container.dataset.keyboardRuntime = "unavailable";
    },
  );

  return {
    ready,
    bindings() {
      return { ...bindings };
    },
    destroy() {
      destroyed = true;
      detachRuntime();
    },
  };
}
