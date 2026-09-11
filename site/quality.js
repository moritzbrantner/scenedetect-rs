const QUALITY_STATUS_ID = "quality-status";
const QUALITY_META_ID = "quality-meta";
const QUALITY_ROWS_ID = "quality-case-rows";
const DIVERGENCES_ID = "quality-divergences";
const QUALITY_RAW_LINK_ID = "quality-raw-link";

function formatDate(value) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("en", {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date);
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
  const totals = snapshot.totals;
  const status = document.getElementById(QUALITY_STATUS_ID);
  if (totals.false_positives === 0 && totals.false_negatives === 0) {
    status.textContent =
      `Across ${totals.cases} deterministic cases, every Reference Oracle boundary was matched within each case's declared tolerance: ` +
      `${totals.matched_boundaries} matched boundaries, no false positives, and no false negatives.`;
  } else {
    status.textContent =
      `This snapshot contains actionable divergence evidence: ${totals.matched_boundaries} matched boundaries, ` +
      `${totals.false_positives} false positives, and ${totals.false_negatives} false negatives across ${totals.cases} cases.`;
  }
}

function renderMeta(snapshot) {
  const meta = document.getElementById(QUALITY_META_ID);
  meta.replaceChildren();
  addMetaRow(meta, "Published", `${formatDate(snapshot.generated_at)} UTC`);
  addMetaRow(meta, "Candidate ref", snapshot.candidate_ref);
  addMetaRow(meta, "Reference Oracle", snapshot.reference_oracle);
  addMetaRow(meta, "Manifest", snapshot.source.manifest);
  addMetaRow(meta, "Machine label", snapshot.source.machine_label);
  addMetaRow(meta, "Command", snapshot.source.command);
  addMetaRow(meta, "Evidence class", snapshot.source.report_only ? "Report-only quality discovery" : "Unknown");
}

function renderCases(snapshot) {
  const rows = document.getElementById(QUALITY_ROWS_ID);
  rows.replaceChildren();
  for (const item of snapshot.cases) {
    const row = document.createElement("tr");
    const values = [
      item.id,
      item.detector,
      item.configuration.tolerance_frames,
      item.reference_boundary_count,
      item.candidate_boundary_count,
      item.matched_boundary_count,
      item.false_positive_count,
      item.false_negative_count,
      item.max_absolute_delta_frames,
    ];
    for (const value of values) {
      const cell = document.createElement("td");
      cell.textContent = String(value);
      row.append(cell);
    }
    const reproduce = document.createElement("td");
    const code = document.createElement("code");
    code.textContent = item.reproduction_command;
    reproduce.append(code);
    row.append(reproduce);
    rows.append(row);
  }
}

function renderDivergences(snapshot) {
  const container = document.getElementById(DIVERGENCES_ID);
  container.replaceChildren();
  if (snapshot.worst_divergences.length === 0) {
    const paragraph = document.createElement("p");
    paragraph.textContent =
      "No ranked divergences are present in this snapshot. This means the current generated corpus is exact within its declared per-case tolerances; it does not imply universal scene-detection equivalence on arbitrary video.";
    container.append(paragraph);
    return;
  }

  const list = document.createElement("ol");
  list.className = "divergence-list";
  for (const item of snapshot.worst_divergences) {
    const entry = document.createElement("li");
    const title = document.createElement("strong");
    title.textContent = `${item.case} · ${item.kind}`;
    const details = document.createElement("p");
    const position = item.timecode ? ` at ${item.timecode}` : "";
    const delta = item.absolute_delta_frames !== undefined ? ` · ${item.absolute_delta_frames} frame delta` : "";
    details.textContent = `${item.detector}${position}${delta}`;
    const reproduce = document.createElement("code");
    reproduce.textContent = item.reproduction_command;
    entry.append(title, details, reproduce);
    list.append(entry);
  }
  container.append(list);
}

function renderUnavailable() {
  document.getElementById(QUALITY_STATUS_ID).textContent =
    "Quality evidence is unavailable. No quality claim is shown without a committed generated-quality snapshot.";
  document.getElementById(QUALITY_META_ID).replaceChildren();
  document.getElementById(QUALITY_ROWS_ID).innerHTML = '<tr><td colspan="10">Quality data is unavailable.</td></tr>';
  document.getElementById(DIVERGENCES_ID).textContent = "Quality divergence data is unavailable.";
}

async function loadQuality() {
  try {
    const response = await fetch("data/quality.json", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const snapshot = await response.json();
    if (snapshot?.schema_version !== 1 || !snapshot.source?.report_only || !Array.isArray(snapshot.cases) || !Array.isArray(snapshot.worst_divergences)) {
      throw new Error("Unsupported quality snapshot schema");
    }
    renderStatus(snapshot);
    renderMeta(snapshot);
    renderCases(snapshot);
    renderDivergences(snapshot);
    document.getElementById(QUALITY_RAW_LINK_ID).href = "data/quality.json";
  } catch (error) {
    renderUnavailable();
  }
}

loadQuality();
