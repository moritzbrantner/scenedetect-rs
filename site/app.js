const SELECT_ID = "benchmark-snapshot";
const DESCRIPTION_ID = "benchmark-description";
const STATUS_ID = "benchmark-status";
const META_ID = "benchmark-meta";
const ROWS_ID = "benchmark-rows";
const RAW_LINK_ID = "benchmark-raw-link";

const corpusOrder = new Map([
  ["generated", 0],
  ["real", 1],
]);

function formatSeconds(value) {
  if (!Number.isFinite(value)) return "n/a";
  if (value < 1) return `${(value * 1000).toFixed(1)} ms`;
  return `${value.toFixed(3)} s`;
}

function formatDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en", {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date);
}

function snapshotAgeDays(value) {
  const generated = new Date(value);
  if (Number.isNaN(generated.getTime())) return null;
  return Math.max(0, Math.floor((Date.now() - generated.getTime()) / 86_400_000));
}

function ratioLabel(caseData) {
  const ratio = Number(caseData.ratio);
  if (!Number.isFinite(ratio) || ratio === 0) return "n/a";
  if (caseData.winner === "candidate") return `${ratio.toFixed(2)}x faster`;
  if (caseData.winner === "reference") return `${(1 / ratio).toFixed(2)}x slower`;
  return "tie";
}

function winnerLabel(winner) {
  if (winner === "candidate") return "scenedetect-rs";
  if (winner === "reference") return "PySceneDetect";
  return "Tie";
}

function addMetaRow(meta, term, description) {
  const row = document.createElement("div");
  const dt = document.createElement("dt");
  const dd = document.createElement("dd");
  dt.textContent = term;
  dd.textContent = description;
  row.append(dt, dd);
  meta.append(row);
}

function renderStatus(entry, snapshot) {
  const status = document.getElementById(STATUS_ID);
  status.className = "benchmark-status";
  const notes = [];
  const ageDays = snapshotAgeDays(snapshot.generated_at);

  if (entry.evidence_class === "current-generated") {
    notes.push("Current deterministic generated-corpus evidence.");
  } else {
    status.classList.add("historical");
    notes.push("Historical evidence retained for coverage not present in the current generated run.");
  }

  if (ageDays !== null) notes.push(`Published ${ageDays} day${ageDays === 1 ? "" : "s"} ago.`);
  if (snapshot.candidate_ref.endsWith("-dirty")) {
    status.classList.add("historical");
    notes.push("The Candidate ref records a dirty working tree; do not read this as current main performance.");
  }
  notes.push("Timing is report-only; correctness remains owned by tests and required parity cases.");
  status.textContent = notes.join(" ");
}

function renderMeta(snapshot) {
  const meta = document.getElementById(META_ID);
  meta.replaceChildren();
  const corpora = Array.from(new Set(snapshot.cases.map((item) => item.corpus))).sort(
    (left, right) => (corpusOrder.get(left) ?? 99) - (corpusOrder.get(right) ?? 99),
  );
  addMetaRow(meta, "Published", `${formatDate(snapshot.generated_at)} UTC`);
  addMetaRow(meta, "Candidate ref", snapshot.candidate_ref);
  addMetaRow(meta, "Reference Oracle", snapshot.reference_oracle);
  addMetaRow(meta, "Corpus coverage", corpora.join(" + ") || "No Benchmark Cases published");
  addMetaRow(meta, "Machine label", snapshot.source.machine_label);
  addMetaRow(meta, "Command", snapshot.source.command);
  addMetaRow(meta, "Timing settings", `${snapshot.settings.warmup} warmup, ${snapshot.settings.runs} measured runs`);
  addMetaRow(meta, "Source note", snapshot.source.notes);
}

function sortCases(cases) {
  return [...cases].sort((left, right) => {
    const corpusDelta = (corpusOrder.get(left.corpus) ?? 99) - (corpusOrder.get(right.corpus) ?? 99);
    return corpusDelta || left.id.localeCompare(right.id);
  });
}

function renderRows(snapshot) {
  const rows = document.getElementById(ROWS_ID);
  rows.replaceChildren();
  if (snapshot.cases.length === 0) {
    rows.innerHTML = '<tr><td colspan="7">No Benchmark Cases are published in this snapshot.</td></tr>';
    return;
  }

  for (const caseData of sortCases(snapshot.cases)) {
    const row = document.createElement("tr");
    const id = document.createElement("td");
    id.className = "case-id";
    id.textContent = caseData.id;
    const corpus = document.createElement("td");
    corpus.textContent = caseData.corpus;
    const detector = document.createElement("td");
    detector.textContent = caseData.detector;
    const reference = document.createElement("td");
    reference.textContent = formatSeconds(Number(caseData.reference_mean_seconds));
    const candidate = document.createElement("td");
    candidate.textContent = formatSeconds(Number(caseData.candidate_mean_seconds));
    const ratio = document.createElement("td");
    ratio.textContent = ratioLabel(caseData);
    const winner = document.createElement("td");
    winner.className = `winner-${caseData.winner}`;
    winner.textContent = winnerLabel(caseData.winner);
    row.append(id, corpus, detector, reference, candidate, ratio, winner);
    rows.append(row);
  }
}

function validateSnapshot(snapshot) {
  if (!snapshot || snapshot.schema_version !== 1 || !Array.isArray(snapshot.cases)) {
    throw new Error("Unsupported benchmark snapshot schema.");
  }
  if (!snapshot.source || !snapshot.settings) throw new Error("Benchmark provenance is incomplete.");
}

async function fetchJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response.json();
}

async function renderSnapshot(entry) {
  const snapshot = await fetchJson(`data/${entry.path}`);
  validateSnapshot(snapshot);
  document.getElementById(DESCRIPTION_ID).textContent = entry.description;
  const rawLink = document.getElementById(RAW_LINK_ID);
  rawLink.href = `data/${entry.path}`;
  rawLink.textContent = `Open raw snapshot: ${entry.label}`;
  renderStatus(entry, snapshot);
  renderMeta(snapshot);
  renderRows(snapshot);
}

function selectedSnapshotId(historyManifest) {
  const requested = new URLSearchParams(window.location.search).get("snapshot");
  return historyManifest.snapshots.some((entry) => entry.id === requested)
    ? requested
    : historyManifest.default_snapshot;
}

function renderSelector(historyManifest) {
  const select = document.getElementById(SELECT_ID);
  select.replaceChildren();
  for (const entry of historyManifest.snapshots) {
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = entry.label;
    select.append(option);
  }
  select.value = selectedSnapshotId(historyManifest);
  select.addEventListener("change", async () => {
    const entry = historyManifest.snapshots.find((item) => item.id === select.value);
    if (!entry) return;
    const url = new URL(window.location.href);
    url.searchParams.set("snapshot", entry.id);
    window.history.replaceState({}, "", url);
    await renderSnapshot(entry);
  });
}

function renderUnavailable() {
  document.getElementById(DESCRIPTION_ID).textContent = "Benchmark evidence is unavailable.";
  const status = document.getElementById(STATUS_ID);
  status.className = "benchmark-status historical";
  status.textContent = "No benchmark claims are shown because the committed evidence could not be loaded.";
  document.getElementById(META_ID).replaceChildren();
  document.getElementById(ROWS_ID).innerHTML = '<tr><td colspan="7">Benchmark data is unavailable.</td></tr>';
}

async function loadBenchmarkHistory() {
  try {
    const historyManifest = await fetchJson("data/benchmark-history.json");
    if (historyManifest?.schema_version !== 1 || !Array.isArray(historyManifest.snapshots)) {
      throw new Error("Unsupported benchmark history schema");
    }
    renderSelector(historyManifest);
    const entry = historyManifest.snapshots.find((item) => item.id === selectedSnapshotId(historyManifest));
    if (!entry) throw new Error("Default benchmark snapshot is missing");
    await renderSnapshot(entry);
  } catch (error) {
    renderUnavailable();
  }
}

loadBenchmarkHistory();
