const STORAGE_KEY = "scenedetect-rs.workbench.keyboard.v1";

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

function normalizedKey(event) {
  if (event.key === " ") {
    return "Space";
  }
  if (event.key.length === 1) {
    return event.key;
  }
  return event.key;
}

function isEditableTarget(target) {
  return Boolean(
    target?.closest?.("input, textarea, select, [contenteditable='true'], [contenteditable='']"),
  );
}

export function createKeyboardController({ container, actions }) {
  let bindings = loadBindings();
  const fields = new Map();

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
        if (!key || key === "Dead") {
          return;
        }
        bindings = { ...bindings, [command]: key };
        saveBindings(bindings);
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
      for (const [command, input] of fields) {
        input.value = bindings[command];
      }
    });

    container.append(grid, reset);
  }

  function handleKeydown(event) {
    if (event.defaultPrevented || isEditableTarget(event.target)) {
      return;
    }
    const key = normalizedKey(event);
    const command = Object.entries(bindings).find(([, binding]) => binding === key)?.[0];
    const action = command ? actions[command] : null;
    if (!action) {
      return;
    }
    event.preventDefault();
    action();
  }

  document.addEventListener("keydown", handleKeydown);
  render();

  return {
    bindings() {
      return { ...bindings };
    },
    destroy() {
      document.removeEventListener("keydown", handleKeydown);
    },
  };
}
