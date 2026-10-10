// Browser acceptance for the GitHub Pages scene lab (#152).
//
// Runs against the built static site (served by serve-static.mjs, no backend)
// and drives the workbench the way a visitor does: open the page, wait for the
// Rust/WASM worker, choose the committed fixture video, analyze it, and read the
// Scene List and boundary review the page renders.
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixturePath = path.join(here, "fixtures", "three-shot-cuts.webm");
const contract = JSON.parse(
  readFileSync(path.join(here, "fixtures", "three-shot-cuts.expected.json"), "utf8"),
);
const fps = contract.workbench_defaults.analysis_fps;

const WASM_READY = "SceneDetect WebAssembly worker loaded.";

function parseClock(text) {
  const [hours, minutes, seconds] = text.trim().split(":").map(Number);
  return hours * 3600 + minutes * 60 + seconds;
}

// Pages publishes a static site with no backend. The suite blocks every request
// that leaves the local static server, so it is hermetic and any runtime
// dependency on another origin shows up as an unavailable feature.
const isolatedContexts = new WeakSet();

async function isolateFromNetwork(page) {
  if (isolatedContexts.has(page.context())) {
    return;
  }
  isolatedContexts.add(page.context());
  await page.context().route(
    (url) => url.hostname !== "127.0.0.1",
    (route) => route.abort("blockedbyclient"),
  );
}

async function openLab(page) {
  await isolateFromNetwork(page);
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto("/workbench.html");
  await expect(page.getByRole("heading", { name: "Run SceneDetect in your browser" })).toBeVisible();
  return pageErrors;
}

async function analyzeFixture(page) {
  await expect(page.locator("#analysis-status")).toContainText(WASM_READY);
  // Before a video is chosen the run control must not look actionable.
  await expect(page.locator("#run-analysis")).toBeDisabled();
  await expect(page.locator("#analysis-status")).toContainText("Choose a local video");

  await expect(page.locator("#detector")).toHaveValue(contract.workbench_defaults.detector);
  await expect(page.locator("#analysis-fps")).toHaveValue(String(fps));

  await page.locator("#video-file").setInputFiles(fixturePath);
  await expect(page.locator("#video-meta")).toContainText("64×36");
  await expect(page.locator("#run-analysis")).toBeEnabled();
  await page.locator("#run-analysis").click();
  await expect(page.locator("#analysis-status")).toHaveText(
    "Analysis complete. Timeline, review controls, and exports are ready.",
    { timeout: 60_000 },
  );
  await expect(page.locator("#results")).toBeVisible();
}

test("the lab loads its Rust WASM worker and detects the fixture's hard cuts", async ({ page }) => {
  const pageErrors = await openLab(page);
  await analyzeFixture(page);

  const { expected } = contract;
  await expect(page.locator("#result-summary")).toContainText(
    `Rust detected ${expected.scene_count} scenes from ${expected.sample_count} browser-decoded samples at ${fps} fps.`,
  );
  await expect(page.locator("#result-summary")).toContainText(
    `${expected.sample_count} presented media timestamps were preserved`,
  );

  // Scene List table: [scene, start sample, start time, end sample, end time, length].
  const rows = page.locator("#scene-rows tr");
  await expect(rows).toHaveCount(expected.scene_count);
  const cells = await rows.evaluateAll((trs) =>
    trs.map((tr) => Array.from(tr.querySelectorAll("td"), (td) => td.textContent)),
  );
  expect(cells.map((row) => Number(row[1]))).toEqual(expected.scene_start_samples);
  // Each Scene ends where the next one starts; the last one ends after the last sample.
  for (let index = 0; index < cells.length - 1; index += 1) {
    expect(Number(cells[index][3])).toBe(expected.scene_start_samples[index + 1]);
  }
  // Boundary media times come from browser-presented frames: the first sampled
  // frame at or after each hard cut, i.e. within one sampling interval of it.
  contract.hard_cut_seconds.forEach((cut, index) => {
    const startSeconds = parseClock(cells[index + 1][2]);
    expect(startSeconds).toBeGreaterThanOrEqual(cut - 0.001);
    expect(startSeconds).toBeLessThanOrEqual(cut + 1 / fps + 0.001);
  });

  // Rust boundary review ranks the accepted Scene Boundaries with their stats.
  await expect(page.locator("#boundary-review")).toBeVisible();
  const candidates = await page.locator("#boundary-rows tr").evaluateAll((trs) =>
    trs.map((tr) => Array.from(tr.querySelectorAll("td"), (td) => td.textContent)),
  );
  const accepted = candidates
    .filter((row) => row[1] === "Accepted")
    .map((row) => ({ frame: Number(row[3]), score: Number(row[5]) }));
  expect(accepted.map((entry) => entry.frame).sort((a, b) => a - b)).toEqual(
    expected.boundary_samples,
  );
  for (const entry of accepted) {
    expect(entry.score).toBeGreaterThan(0);
  }

  // The Detection Stats export carries one stats row per analyzed sample.
  const download = page.waitForEvent("download");
  await page.locator('button[data-export="stats_csv"]').click();
  const stats = readFileSync(await (await download).path(), "utf8").trim().split(/\r?\n/u);
  expect(stats.length - 1).toBe(expected.sample_count);

  expect(pageErrors).toEqual([]);
});

async function startProbe(page) {
  await page.evaluate(() => {
    const video = document.getElementById("video-preview");
    window.__probe = { mutations: 0, time: video.currentTime };
    window.__probeObserver?.disconnect();
    window.__probeObserver = new MutationObserver((records) => {
      window.__probe.mutations += records.length;
    });
    window.__probeObserver.observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
  });
}

async function stopProbe(page) {
  return page.evaluate(() => {
    window.__probeObserver.disconnect();
    const video = document.getElementById("video-preview");
    return {
      mutations: window.__probe.mutations,
      seeked: Math.abs(video.currentTime - window.__probe.time) > 1e-6,
    };
  });
}

async function analyzedQuietLab(page) {
  await openLab(page);
  await analyzeFixture(page);
  // Deterministic starting state: playhead at the start, nothing selected,
  // and no DOM activity while nobody interacts with the page.
  await page.evaluate(async () => {
    const video = document.getElementById("video-preview");
    video.pause();
    if (video.currentTime !== 0) {
      const seeked = new Promise((resolve) => video.addEventListener("seeked", resolve, { once: true }));
      video.currentTime = 0;
      await seeked;
    }
  });
  for (let attempt = 0; ; attempt += 1) {
    await startProbe(page);
    await page.waitForTimeout(400);
    const idle = await stopProbe(page);
    if (idle.mutations === 0 && !idle.seeked) {
      return;
    }
    expect(attempt, "the analyzed lab never settles, so button effects cannot be observed").toBeLessThan(10);
  }
}

test("controls in the review area are never dead: each visible enabled button has an effect", async ({
  page,
}) => {
  // Every button is probed from the same freshly analyzed state, so one
  // button's effect cannot mask another button that does nothing.
  test.setTimeout(10 * 60_000);
  await analyzedQuietLab(page);
  const total = await page.locator("#results button").count();
  expect(total).toBeGreaterThan(0);
  const dead = [];
  let probed = 0;

  for (let index = 0; index < total; index += 1) {
    if (index > 0) {
      await analyzedQuietLab(page);
    }
    const button = page.locator("#results button").nth(index);
    if (!(await button.isVisible()) || (await button.isDisabled())) {
      continue;
    }
    const label =
      (await button.getAttribute("aria-label")) ||
      (await button.getAttribute("title")) ||
      (await button.textContent()).trim() ||
      `#results button #${index}`;
    await startProbe(page);
    let sideEffect = false;
    const onEvent = () => {
      sideEffect = true;
    };
    page.on("download", onEvent);
    page.on("filechooser", onEvent);
    await button.click();
    await page.waitForTimeout(400);
    page.off("download", onEvent);
    page.off("filechooser", onEvent);
    const observed = await stopProbe(page);
    probed += 1;
    if (!sideEffect && observed.mutations === 0 && !observed.seeked) {
      dead.push(label);
    }
  }

  expect(probed).toBeGreaterThan(0);
  // An action that is unavailable in the current state must be disabled or
  // explain itself; an enabled button that silently does nothing is a defect.
  expect(dead, `enabled buttons with no observable effect: ${dead.join(", ")}`).toEqual([]);
});

test("a broken WASM module is reported explicitly and leaves no actionable run control", async ({
  page,
}) => {
  await page.context().route("**/wasm/scenedetect_wasm.wasm", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/wasm",
      body: Buffer.from("this is not a WebAssembly module"),
    }),
  );
  await openLab(page);

  const status = page.locator("#analysis-status");
  await expect(status).toContainText("Unable to load SceneDetect WebAssembly worker");
  await expect(status).not.toContainText(WASM_READY);

  await page.locator("#video-file").setInputFiles(fixturePath);
  await expect(page.locator("#video-meta")).toContainText("64×36");
  await expect(page.locator("#run-analysis")).toBeDisabled();
  await expect(page.locator("#results")).toBeHidden();
});

test("a missing WASM module is reported explicitly", async ({ page }) => {
  await page.context().route("**/wasm/scenedetect_wasm.wasm", (route) =>
    route.fulfill({ status: 404, body: "Not found" }),
  );
  await openLab(page);
  await expect(page.locator("#analysis-status")).toContainText(
    "Unable to load SceneDetect WebAssembly worker: Unable to load SceneDetect WASM (HTTP 404).",
  );
  await expect(page.locator("#run-analysis")).toBeDisabled();
});

test("keyboard shortcuts report an unavailable runtime instead of offering dead controls", async ({
  page,
}) => {
  // The shortcut runtime is imported from another origin at run time. When it
  // cannot load (here: the hermetic suite blocks every non-local request), the
  // workbench must say so visibly and must not offer shortcut editing that has
  // no effect.
  const runtimeRequests = [];
  page.on("request", (request) => {
    if (new URL(request.url()).hostname !== "127.0.0.1") {
      runtimeRequests.push(request.url());
    }
  });
  await openLab(page);
  await analyzeFixture(page);

  const panel = page.locator(".keyboard-panel");
  await expect(panel).toBeVisible();
  if (runtimeRequests.length === 0) {
    // The workbench no longer depends on another origin for shortcuts; the
    // controls are then self-contained and this unavailable path is moot.
    return;
  }
  await expect(panel).toContainText(/unavailable/i);
  const shortcutInputs = page.locator("#keyboard-bindings input");
  for (let index = 0; index < (await shortcutInputs.count()); index += 1) {
    await expect(shortcutInputs.nth(index)).toBeDisabled();
  }
  const reset = page.locator("#keyboard-bindings button", { hasText: "Reset shortcuts" });
  if (await reset.isVisible()) {
    await expect(reset).toBeDisabled();
  }
});
