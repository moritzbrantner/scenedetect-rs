const SUMMARY_ID = "benchmark-summary";
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

function setSummary(snapshot) {
  const summary = document.getElementById(SUMMARY_ID);
  const corpora = new Set(snapshot.cases.map((caseData) => caseData.corpus));
  const corpusLabel = Array.from(corpora).sort().join(" + ") || "no";
  summary.textContent = [
    `${snapshot.cases.length} Benchmark Cases`,
    `${corpusLabel} corpus`,
    `Reference Oracle: ${snapshot.reference_oracle}`,
    `Candidate: ${snapshot.candidate_ref}`,
    `Generated: ${snapshot.generated_at}`,
    snapshot.source.notes,
  ].join(" | ");
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

  if (!Array.isArray(snapshot.cases) || snapshot.cases.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.textContent =
      "No benchmark cases are published yet. Run the local snapshot workflow to populate this table.";
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
}

async function loadBenchmarks() {
  const summary = document.getElementById(SUMMARY_ID);
  const rows = document.getElementById(ROWS_ID);

  try {
    const response = await fetch("data/benchmarks.json", { cache: "no-store" });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const snapshot = await response.json();
    validateSnapshot(snapshot);
    setSummary(snapshot);
    renderRows(snapshot);
  } catch (error) {
    summary.textContent =
      "Benchmark snapshot could not be loaded. The site remains usable, but timing data is unavailable.";
    rows.innerHTML = '<tr><td colspan="7">Benchmark data is unavailable.</td></tr>';
  }
}

loadBenchmarks();
