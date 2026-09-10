const STATUS_ID = "benchmark-status";
const META_ID = "benchmark-meta";
const ROWS_ID = "benchmark-rows";

const corpusOrder = new Map([
  ["generated", 0],
  ["real", 1],
]);

function formatSeconds(value) {
  if (!Number.isFinite(value)) {
    return "n/a";
  }
  if (value < 1) {
    return `${(value * 1000).toFixed(1)} ms`;
  }
  return `${value.toFixed(3)} s`;
}

function formatDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("en", {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date);
}

function snapshotAgeDays(value) {
  const generated = new Date(value);
  if (Number.isNaN(generated.getTime())) {
    return null;
  }
  const ageMs = Date.now() - generated.getTime();
  return Math.max(0, Math.floor(ageMs / 86_400_000));
}

function ratioLabel(caseData) {
  const ratio = Number(caseData.ratio);
  if (!Number.isFinite(ratio) || ratio === 0) {
    return "n/a";
  }
  if (caseData.winner === "candidate") {
    return `${ratio.toFixed(2)}x faster`;
  }
  if (caseData.winner === "reference") {
    return `${(1 / ratio).toFixed(2)}x slower`;
  }
  return "tie";
}

function winnerLabel(winner) {
  if (winner === "candidate") {
    return "scenedetect-rs";
  }
  if (winner === "reference") {
    return "PySceneDetect";
  }
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

function renderStatus(snapshot) {
  const status = document.getElementById(STATUS_ID);
  const ageDays = snapshotAgeDays(snapshot.generated_at);
  const dirtyCandidate = snapshot.candidate_ref.endsWith("-dirty");
  const notes = [];

  if (ageDays === null) {
    notes.push("Snapshot age is unavailable.");
  } else if (ageDays > 45) {
    status.classList.add("historical");
    notes.push(`This is historical performance evidence: the snapshot is ${ageDays} days old.`);
  } else {
    notes.push(`This snapshot was published ${ageDays} day${ageDays === 1 ? "" : "s"} ago.`);
  }

  if (dirtyCandidate) {
    status.classList.add("historical");
    notes.push("Its Candidate ref records a dirty working tree, so it must not be read as current main performance.");
  }

  notes.push("Use required tests and parity cases for correctness claims.");
  status.textContent = notes.join(" ");
}

function renderMeta(snapshot) {
  const meta = document.getElementById(META_ID);
  meta.replaceChildren();

  const corpora = Array.from(new Set(snapshot.cases.map((caseData) => caseData.corpus))).sort(
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
    const corpusDelta =
      (corpusOrder.get(left.corpus) ?? 99) - (corpusOrder.get(right.corpus) ?? 99);
    if (corpusDelta !== 0) {
      return corpusDelta;
    }
    return left.id.localeCompare(right.id);
  });
}

function renderRows(snapshot) {
  const rows = document.getElementById(ROWS_ID);
  rows.replaceChildren();

  if (snapshot.cases.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.textContent =
      "No Benchmark Cases are published yet. Run the deliberate local snapshot workflow to populate this table.";
    row.append(cell);
    rows.append(row);
    return;
  }

  for (const caseData of sortCases(snapshot.cases)) {
    const row = document.createElement("tr");

    const id = document.createElement("td");
    id.className = "case-id";
    id.textContent = caseData.id;

    const corpus = document.createElement("td");
    const corpusBadge = document.createElement("span");
    corpusBadge.className = `badge ${caseData.corpus}`;
    corpusBadge.textContent = caseData.corpus;
    corpus.append(corpusBadge);

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
  if (!snapshot.source || !snapshot.settings) {
    throw new Error("Benchmark snapshot provenance is incomplete.");
  }
}

function renderUnavailable() {
  const status = document.getElementById(STATUS_ID);
  const meta = document.getElementById(META_ID);
  const rows = document.getElementById(ROWS_ID);

  status.classList.add("historical");
  status.textContent =
    "Benchmark data is unavailable. The project site remains usable, but this page cannot make timing claims without its committed snapshot.";

  meta.replaceChildren();
  addMetaRow(meta, "Snapshot", "Unavailable");

  rows.replaceChildren();
  const row = document.createElement("tr");
  const cell = document.createElement("td");
  cell.colSpan = 7;
  cell.textContent = "Benchmark data is unavailable.";
  row.append(cell);
  rows.append(row);
}

async function loadBenchmarks() {
  try {
    const response = await fetch("data/benchmarks.json", { cache: "no-store" });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const snapshot = await response.json();
    validateSnapshot(snapshot);
    renderStatus(snapshot);
    renderMeta(snapshot);
    renderRows(snapshot);
  } catch (error) {
    renderUnavailable();
  }
}

loadBenchmarks();
