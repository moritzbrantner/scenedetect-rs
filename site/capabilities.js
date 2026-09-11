const DETECTOR_ROWS_ID = "capability-detector-rows";
const SURFACES_ID = "capability-surfaces";
const SOURCE_ID = "capability-source";

function labelStatus(status) {
  if (status === "supported") return "Supported";
  if (status === "not-claimed") return "Not claimed";
  if (status === "unsupported") return "Unsupported";
  return status;
}

function appendTextCell(row, text, className = "") {
  const cell = document.createElement("td");
  if (className) cell.className = className;
  cell.textContent = text;
  row.append(cell);
  return cell;
}

function renderDetectors(manifest) {
  const rows = document.getElementById(DETECTOR_ROWS_ID);
  rows.replaceChildren();

  for (const detector of manifest.detectors) {
    const row = document.createElement("tr");
    const command = document.createElement("td");
    const code = document.createElement("code");
    code.textContent = detector.command;
    command.append(code);
    row.append(command);
    appendTextCell(row, labelStatus(detector.status), "status-text");
    appendTextCell(row, detector.parity === "required" ? "Required" : detector.parity);
    appendTextCell(row, detector.verified_behavior);

    const evidence = document.createElement("td");
    const list = document.createElement("ul");
    list.className = "evidence-case-list";
    for (const caseId of detector.case_ids) {
      const item = document.createElement("li");
      const link = document.createElement("a");
      link.href = `https://github.com/moritzbrantner/scenedetect-rs/blob/main/tests/parity/cases.toml`;
      link.textContent = caseId;
      item.append(link);
      list.append(item);
    }
    evidence.append(list);
    row.append(evidence);
    rows.append(row);
  }
}

function renderSurfaces(manifest) {
  const container = document.getElementById(SURFACES_ID);
  container.replaceChildren();

  for (const surface of manifest.surfaces) {
    const entry = document.createElement("div");
    const term = document.createElement("dt");
    const description = document.createElement("dd");
    const status = document.createElement("span");
    status.className = `evidence-status ${surface.status}`;
    status.textContent = labelStatus(surface.status);
    term.textContent = surface.name;
    term.append(" ", status);

    const guarantee = document.createElement("p");
    guarantee.textContent = surface.guarantee;
    const ownership = document.createElement("p");
    ownership.className = "evidence-provenance";
    ownership.textContent = `Owner: ${surface.owner}`;
    const link = document.createElement("a");
    link.href = surface.evidence_href;
    link.textContent = "Open supporting evidence";
    description.append(guarantee, ownership, link);
    entry.append(term, description);
    container.append(entry);
  }
}

function renderSource(manifest) {
  const source = document.getElementById(SOURCE_ID);
  source.textContent = `Detector support is checked against ${manifest.evidence_source} using ${manifest.reference_oracle} as the Reference Oracle.`;
}

function renderUnavailable() {
  document.getElementById(SOURCE_ID).textContent =
    "Capability evidence is unavailable. No support claims are rendered without the committed manifest.";
  const rows = document.getElementById(DETECTOR_ROWS_ID);
  rows.innerHTML = '<tr><td colspan="5">Capability data is unavailable.</td></tr>';
  document.getElementById(SURFACES_ID).replaceChildren();
}

async function loadCapabilities() {
  try {
    const response = await fetch("data/capabilities.json", { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const manifest = await response.json();
    if (manifest?.schema_version !== 1 || !Array.isArray(manifest.detectors) || !Array.isArray(manifest.surfaces)) {
      throw new Error("Unsupported capability manifest schema");
    }
    renderSource(manifest);
    renderDetectors(manifest);
    renderSurfaces(manifest);
  } catch (error) {
    renderUnavailable();
  }
}

loadCapabilities();
