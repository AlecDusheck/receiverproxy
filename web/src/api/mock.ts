// Canned daemon for `VITE_RXP_MOCK=1 pnpm dev`: one card, in-memory settings and wall, simulated jobs.
import type { Canvas, Job, JobKind, Line } from "./types";
import { single } from "../lib/state.svelte";

const card = { controller: 0, card_id: 0x64, model: "E120", ver_major: 16, ver_minor: 53, cols: 128, rows: 64 };
let settings = { iface: "en24", brightness: 255, card: null as string | null };
let wall: Canvas = single(128, 64);
const jobs = new Map<string, Job & { listeners: ((l: Line) => void)[]; enders: ((j: Job) => void)[]; timer: number }>();
let counter = 0;

const outcome = (lines: string[]) => ({
  lines: lines.map((text) => ({ stream: "err" as const, text })),
  files: [] as string[],
});
const gatedOutcome = (lines: string[], committed: boolean) => ({ ...outcome(lines), committed });

function fail(status: number, error: string): never {
  const e = new Error(error) as Error & { status: number };
  e.status = status;
  throw e;
}

function startJob(kind: JobKind, script: string[], commit: boolean) {
  const running = [...jobs.values()].find((j) => j.state === "running");
  if (running && running.kind !== "show/video" && running.kind !== "show/hold") fail(409, `job ${running.id} (${running.kind}) is running`);
  if (running) cancel(running.id);
  const id = `j${++counter}`;
  const job = {
    id,
    kind,
    state: "running" as Job["state"],
    started: new Date().toISOString(),
    finished: null as string | null,
    lines: [] as Line[],
    error: null as string | null,
    result: null as Job["result"],
    listeners: [] as ((l: Line) => void)[],
    enders: [] as ((j: Job) => void)[],
    timer: 0,
  };
  jobs.set(id, job);
  let i = 0;
  const step = () => {
    if (job.state !== "running") return;
    if (i < script.length) {
      const l = { stream: "err" as const, text: script[i++]! };
      job.lines.push(l);
      job.listeners.forEach((f) => f(l));
      job.timer = window.setTimeout(step, 600);
    } else {
      job.state = "done";
      job.finished = new Date().toISOString();
      job.result = kind === "show/video" || kind === "show/hold" || kind === "flash/snapshot" ? outcome([]) : gatedOutcome([], commit);
      job.enders.forEach((f) => f(strip(job)));
    }
  };
  job.timer = window.setTimeout(step, 300);
  return { id };
}

function cancel(id: string): Job {
  const job = jobs.get(id) ?? fail(404, `job ${id}: not found`);
  if (job.state === "running") {
    clearTimeout(job.timer);
    job.state = "cancelled";
    job.finished = new Date().toISOString();
    job.enders.forEach((f) => f(strip(job)));
  }
  return strip(job);
}

const strip = (j: Job & { listeners?: unknown; enders?: unknown; timer?: unknown }): Job => {
  const { listeners: _l, enders: _e, timer: _t, ...rest } = j;
  return rest as Job;
};

const provisionScript = (commit: boolean) => [
  "[1/5] snapshot: ~/Library/Application Support/receiverproxy/snapshots/1756771200",
  "[2/5] firmware: skipped (no image given)",
  "[3/5] eeprom: read 256 bytes",
  "[4/5] config: 21 records, 2 pages",
  commit ? "[5/5] eeprom: written, verified" : "[5/5] eeprom: would write position 0,0 (dry run, pass commit)",
];

export async function request(method: string, path: string, body?: unknown): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120));
  const b = (body ?? {}) as Record<string, unknown>;
  const commit = b.commit === true;
  const key = `${method} ${path.split("?")[0]}`;
  switch (key) {
    case "GET /health":
      return { version: "0.1.0-mock", iface: settings.iface, cards: [card] };
    case "POST /discover":
      return { cards: [card] };
    case "GET /settings":
      return settings;
    case "PUT /settings":
      settings = b as typeof settings;
      return settings;
    case "POST /brightness":
      settings.brightness = Number(b.value);
      return { value: settings.brightness };
    case "POST /show/image":
    case "POST /show/pattern":
    case "POST /show/fill": {
      const hold = body instanceof FormData ? body.get("hold") === "true" : b.hold === true;
      return hold ? startJob("show/hold", ["holding, 3 refreshes/s"], false) : outcome(["3 refreshes sent"]);
    }
    case "POST /show/blank":
      return outcome(["3 black refreshes sent"]);
    case "POST /show/video":
      return startJob("show/video", ["60 frames, 30.0 fps", "120 frames, 30.0 fps", "180 frames, 30.0 fps"], false);
    case "POST /config/read":
      return { rcvbp: btoa("RCVB mock"), lines: [{ stream: "err", text: "read 64 chunks" }] };
    case "POST /config/write":
      return gatedOutcome([commit ? "block 7: written, verified" : "block 7: would write 65536 bytes (dry run)"], commit);
    case "POST /config/send":
      return outcome(["sent 21 records to RAM"]);
    case "POST /provision":
      return startJob("provision", provisionScript(commit), commit);
    case "POST /flash/snapshot":
      return startJob("flash/snapshot", ["block 0/8", "block 4/8", "block 8/8", "golden bank: 8 blocks"], false);
    case "POST /flash/restore":
      return startJob("flash/restore", [commit ? "block 7: written" : "block 7: would restore (dry run)"], commit);
    case "POST /firmware/install":
      return startJob("firmware/install", ["staging 1024/4096", "staging 4096/4096", commit ? "activated" : "dry run: not activated"], commit);
    case "GET /card/screen-size":
      return { width: 128, height: 64 };
    case "PUT /card/screen-size":
      return { ...gatedOutcome([commit ? "eeprom: written" : "eeprom: would write (dry run)"], commit), width: b.width, height: b.height };
    case "POST /card/reload":
    case "POST /card/test-mode":
    case "POST /card/set-layout":
      return outcome([`${path.slice(6)}: sent`]);
    case "GET /wall":
      return wall;
    case "PUT /wall":
      wall = b as unknown as Canvas;
      return wall;
    case "GET /jobs":
      return [...jobs.values()].map(strip).reverse();
  }
  const jm = /^(GET|DELETE) \/jobs\/([^/]+)$/.exec(key);
  if (jm) {
    const j = jobs.get(jm[2]!) ?? fail(404, `job ${jm[2]}: not found`);
    return jm[1] === "DELETE" ? cancel(j.id) : strip(j);
  }
  return fail(404, `unknown route ${key}`);
}

export function sse(jobId: string, onLine: (l: Line) => void, onEnd: (j: Job) => void): () => void {
  const job = jobs.get(jobId);
  if (!job) return () => {};
  job.lines.forEach(onLine);
  if (job.state !== "running") {
    setTimeout(() => onEnd(strip(job)));
    return () => {};
  }
  job.listeners.push(onLine);
  job.enders.push(onEnd);
  return () => {
    job.listeners = job.listeners.filter((f) => f !== onLine);
    job.enders = job.enders.filter((f) => f !== onEnd);
  };
}
