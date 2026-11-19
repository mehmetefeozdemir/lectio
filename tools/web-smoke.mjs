// Web smoke test for the trunk build (ADR-009).
//
// Usage: node tools/web-smoke.mjs <url> [step ...]
//        node tools/web-smoke.mjs <url> --flow phase2      (a named flow, see FLOWS)
//   steps run in order after Lectio has started:
//     --shot FILE.png    save a screenshot
//     --click X,Y        left-click at page coordinates (1400×900 window)
//     --type TEXT        type text into the focused field
//     --key NAME         press Enter, Escape or Tab
//     --wait MS          wait (real time)
//     --reload           reload the page and wait until Lectio has started again
//     --upload FILE      the next file chooser the page opens gets FILE (Phase 2 imports)
//     --drag X1,Y1,X2,Y2 press, move in small steps and release (drawing a sketch)
//     --scroll X,Y,DY    mouse wheel at X,Y by DY pixels (positive = down)
//     --expect-stored K=V,…  read the app's IndexedDB and fail unless the counts
//                        match, e.g. notes=3,linked=3,attachments=2,decks=1,blobs=3
//                        (egui draws on a canvas, so the stored data is what we can check)
//   Set CHROME=/path/to/chrome to use another browser binary.
//   (Steps use coordinates because egui draws on a <canvas> and, on the web,
//   does not expose its widgets to the DOM accessibility tree; PROBLEMS P-012.)
//
// Chrome runs headless with a *fake microphone* (a test tone) and the
// microphone permission auto-granted, so recording can be tested too.
// It waits in real time (up to 30 s) for the "Loading Lectio…" line to go
// away, prints the browser console at the end, and exits 0 if every step
// worked. Uses only Node's built-ins (fetch, WebSocket) and the Chrome
// DevTools Protocol, so it needs no npm install.

import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

/**
 * Named flows (`--flow NAME`), so CI and people don't retype long step lists.
 * Coordinates match the default layout in a 1400×900 window (P-010: they
 * break if the layout changes; re-measure from the screenshots).
 */
const SAMPLE_PDF = new URL("../assets/sample-slides.pdf", import.meta.url).pathname;
const FLOWS = {
  // Phase 2 end to end: import a PDF through the real file chooser, record,
  // type a note (auto-linked to slide 1), turn to slide 2, draw and save a
  // sketch, take a photo with Chrome's fake camera, stop, reload the page,
  // reopen the lecture (everything must come back from IndexedDB), then the
  // reading view, scrolled down to the photo.
  phase2: [
    "--click", "95,174", "--type", "CSCA67", "--key", "Enter", "--wait", "500",
    "--click", "85,143", "--wait", "800",
    "--upload", SAMPLE_PDF, "--click", "1325,79", "--wait", "2500",
    "--click", "372,170", "--wait", "1500",
    "--click", "580,225", "--type", "BFS uses a queue", "--key", "Enter", "--wait", "500",
    "--click", "1183,141", "--wait", "800",
    "--click", "733,299", "--wait", "800",
    "--drag", "500,250,900,250", "--drag", "700,250,550,480", "--drag", "700,250,850,480",
    "--click", "563,621", "--type", "Queue diagram", "--wait", "300",
    "--click", "521,658", "--wait", "1000",
    "--click", "624,299", "--wait", "3000", "--shot", "shots/web-flow-camera.png",
    "--click", "429,581", "--wait", "1500",
    "--click", "519,170", "--wait", "2500",
    "--reload", "--wait", "1500",
    // 3 notes (typed, sketch, photo), all linked to a slide; 2 images; 1 deck;
    // 3 blobs (PDF, sketch PNG, photo JPEG), read back after the reload.
    "--expect-stored", "notes=3,linked=3,attachments=2,decks=1,blobs=3",
    "--click", "69,143", "--wait", "3000", "--shot", "shots/web-flow-after-reload.png",
    "--click", "1187,20", "--wait", "1500", "--shot", "shots/web-flow-reading.png",
    "--scroll", "600,500,700", "--scroll", "600,500,700", "--wait", "800", "--shot", "shots/web-flow-reading-photo.png",
  ],
};

const [url, ...rawSteps] = process.argv.slice(2);
const stepArgs = rawSteps[0] === "--flow" ? (FLOWS[rawSteps[1]] ?? []).concat(rawSteps.slice(2)) : rawSteps;
if (rawSteps[0] === "--flow" && !FLOWS[rawSteps[1]]) {
  console.error(`unknown flow ${rawSteps[1]}; known: ${Object.keys(FLOWS).join(", ")}`);
  process.exit(2);
}
if (!url) {
  console.error("usage: node tools/web-smoke.mjs <url> [--shot F] [--click X,Y] [--type T] [--key K] [--wait MS] [--reload] [--upload FILE] [--drag X1,Y1,X2,Y2]");
  process.exit(2);
}
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9333;
const TIMEOUT_MS = 30_000;
const POLL_MS = 250;
const KEYS = {
  Enter: { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" },
  Escape: { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 },
  Tab: { key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 },
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "lectio-smoke-"))}`,
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--use-fake-ui-for-media-stream",
  "--use-fake-device-for-media-stream",
  "--autoplay-policy=no-user-gesture-required",
  "--window-size=1400,900",
  "about:blank",
], { stdio: "ignore" });

let exitCode = 1;
const consoleLines = [];
/** Files queued by `--upload`, handed to file choosers in order. */
const pendingUploads = [];
try {
  const target = await waitForPageTarget();
  const cdp = await connect(target.webSocketDebuggerUrl);
  cdp.onEvent = (msg) => {
    recordConsole(msg);
    if (msg.method === "Page.fileChooserOpened" && pendingUploads.length > 0) {
      const file = pendingUploads.shift();
      console.log(`upload: ${file}`);
      cdp.send("DOM.setFileInputFiles", { files: [file], backendNodeId: msg.params.backendNodeId })
        .catch((e) => consoleLines.push(`upload failed: ${e.message}`));
    }
  };
  await cdp.send("DOM.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Page.enable");
  await cdp.send("Page.navigate", { url });
  const started = await waitForStart(cdp);
  console.log(`start: ${started}`);
  if (started !== "started") throw new Error(started);
  await runSteps(cdp, stepArgs);
  exitCode = 0;
  cdp.close();
} catch (e) {
  console.error(`FAIL: ${e.message}`);
} finally {
  for (const line of consoleLines) console.log(`console ${line}`);
  chrome.kill();
  process.exit(exitCode);
}

function recordConsole(msg) {
  if (msg.method === "Runtime.consoleAPICalled") {
    consoleLines.push(`${msg.params.type}: ${msg.params.args.map((a) => a.value ?? a.description).join(" ")}`);
  }
  if (msg.method === "Runtime.exceptionThrown") {
    const d = msg.params.exceptionDetails;
    consoleLines.push(`exception: ${d.exception?.description ?? d.text}`);
  }
}

async function runSteps(cdp, args) {
  for (let i = 0; i < args.length; i++) {
    const step = args[i];
    const value = args[i + 1];
    switch (step) {
      case "--shot": {
        await sleep(500);
        const shot = await cdp.send("Page.captureScreenshot", { format: "png" });
        mkdirSync(dirname(resolve(value)), { recursive: true });
        writeFileSync(value, Buffer.from(shot.data, "base64"));
        console.log(`screenshot: ${value}`);
        i++;
        break;
      }
      case "--click": {
        const [x, y] = value.split(",").map(Number);
        for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
          await cdp.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
        }
        await sleep(300);
        i++;
        break;
      }
      case "--type":
        await cdp.send("Input.insertText", { text: value });
        await sleep(300);
        i++;
        break;
      case "--key": {
        const key = KEYS[value];
        if (!key) throw new Error(`unknown key ${value}`);
        await cdp.send("Input.dispatchKeyEvent", { type: "keyDown", ...key });
        await cdp.send("Input.dispatchKeyEvent", { type: "keyUp", ...key, text: undefined });
        await sleep(300);
        i++;
        break;
      }
      case "--wait":
        await sleep(Number(value));
        i++;
        break;
      case "--reload": {
        await cdp.send("Page.reload");
        await waitForFreshPage(cdp);
        const again = await waitForStart(cdp);
        console.log(`reload: ${again}`);
        if (again !== "started") throw new Error(again);
        break;
      }
      case "--upload": {
        // Chrome then reports file choosers to us instead of showing them;
        // `recordConsole` hands the queued file to the next one that opens.
        pendingUploads.push(resolve(value));
        await cdp.send("Page.setInterceptFileChooserDialog", { enabled: true });
        i++;
        break;
      }
      case "--drag": {
        const [x1, y1, x2, y2] = value.split(",").map(Number);
        const steps = 12;
        await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x1, y: y1 });
        await cdp.send("Input.dispatchMouseEvent", { type: "mousePressed", x: x1, y: y1, button: "left", clickCount: 1 });
        for (let s = 1; s <= steps; s++) {
          const x = x1 + ((x2 - x1) * s) / steps;
          const y = y1 + ((y2 - y1) * s) / steps;
          await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "left", buttons: 1 });
          await sleep(16); // about one frame per step
        }
        await cdp.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x2, y: y2, button: "left", clickCount: 1 });
        await sleep(200);
        i++;
        break;
      }
      case "--scroll": {
        const [x, y, deltaY] = value.split(",").map(Number);
        // egui scrolls the area under the pointer, so move there first.
        await cdp.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
        await sleep(100);
        await cdp.send("Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY });
        await sleep(400);
        i++;
        break;
      }
      case "--expect-stored": {
        const actual = await storedCounts(cdp);
        console.log(`stored: ${JSON.stringify(actual)}`);
        for (const pair of value.split(",")) {
          const [key, want] = pair.split("=");
          if (actual[key] !== Number(want)) throw new Error(`stored ${key} is ${actual[key]}, expected ${want}`);
        }
        i++;
        break;
      }
      default:
        throw new Error(`unknown step ${step}`);
    }
  }
}

/** Counts what Lectio saved in IndexedDB "lectio" (see src/platform/web/idb.rs). */
async function storedCounts(cdp) {
  const expression = `(async () => {
    const db = await new Promise((ok, fail) => {
      const r = indexedDB.open("lectio");
      r.onsuccess = () => ok(r.result);
      r.onerror = () => fail(r.error);
    });
    const read = (store, op) => new Promise((ok, fail) => {
      const r = op(db.transaction(store).objectStore(store));
      r.onsuccess = () => ok(r.result);
      r.onerror = () => fail(r.error);
    });
    const json = await read("library", (s) => s.get("library.json"));
    const blobs = await read("blobs", (s) => s.count());
    const sessions = JSON.parse(json).courses.flatMap((c) => c.sessions);
    const notes = sessions.flatMap((s) => s.notes);
    return {
      notes: notes.length,
      linked: notes.filter((n) => n.slide).length,
      attachments: notes.filter((n) => n.attachment).length,
      decks: sessions.flatMap((s) => s.decks || []).length,
      blobs,
    };
  })()`;
  const res = await cdp.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (res.exceptionDetails) throw new Error(`reading IndexedDB failed: ${res.exceptionDetails.text}`);
  return res.result.value;
}

async function waitForPageTarget() {
  for (let i = 0; i < 40; i++) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = targets.find((t) => t.type === "page");
      if (page) return page;
    } catch { /* Chrome still starting */ }
    await sleep(POLL_MS);
  }
  throw new Error("Chrome did not open a DevTools port");
}

/** "started" once the status line is gone; "error: …" if it shows a failure. */
async function waitForStart(cdp) {
  const deadline = Date.now() + TIMEOUT_MS;
  while (Date.now() < deadline) {
    const res = await cdp.send("Runtime.evaluate", {
      expression: `(() => { const s = document.getElementById("lectio_status");
                            return s ? s.textContent : "__gone__"; })()`,
      returnByValue: true,
    });
    const text = res.result?.value;
    if (text === "__gone__") return "started";
    if (typeof text === "string" && text.startsWith("Lectio could not start")) return `error: ${text}`;
    await sleep(POLL_MS);
  }
  return "timeout: still loading after 30 s";
}

/** After a reload: wait until the new page shows its "Loading" line again. */
async function waitForFreshPage(cdp) {
  for (let i = 0; i < 40; i++) {
    const res = await cdp.send("Runtime.evaluate", {
      expression: `!!document.getElementById("lectio_status")`,
      returnByValue: true,
    });
    if (res.result?.value === true) return;
    await sleep(POLL_MS / 5);
  }
}

function connect(wsUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    let nextId = 1;
    const pending = new Map();
    const api = {
      onEvent: () => {},
      send(method, params = {}) {
        const id = nextId++;
        ws.send(JSON.stringify({ id, method, params }));
        return new Promise((res, rej) => pending.set(id, { res, rej }));
      },
      close: () => ws.close(),
    };
    ws.onmessage = (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && pending.has(msg.id)) {
        const { res, rej } = pending.get(msg.id);
        pending.delete(msg.id);
        msg.error ? rej(new Error(msg.error.message)) : res(msg.result);
      } else if (msg.method) {
        api.onEvent(msg);
      }
    };
    ws.onopen = () => resolve(api);
    ws.onerror = () => reject(new Error("could not connect to Chrome DevTools"));
  });
}
