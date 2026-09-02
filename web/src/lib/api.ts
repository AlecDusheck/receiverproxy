// The daemon's JSON API (docs/ui.md section 2). One request helper, one SSE helper.
import { app, setStatus } from "./state.svelte";
import type { Canvas, Card, Fit, GatedOutcome, GenFiles, Health, Job, Line, Outcome, PatternName, Settings } from "./types";
import * as mock from "./mock";

const MOCK = import.meta.env.VITE_E120_MOCK === "1";
export const base: string = MOCK ? "mock" : ((import.meta.env.VITE_E120_API as string | undefined) ?? "/api/v1");

let token: string | null = null;
const m = /[#&]token=([^&]+)/.exec(location.hash);
if (m?.[1]) {
  token = decodeURIComponent(m[1]);
  history.replaceState(null, "", location.pathname + location.hash.replace(/[#&]token=[^&]+/, "#"));
}

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown, timeoutMs?: number): Promise<T> {
  if (MOCK) return mock.request(method, path, body) as Promise<T>;
  const headers: Record<string, string> = {};
  if (token) headers["X-Token"] = token;
  let payload: BodyInit | undefined;
  if (body instanceof FormData) payload = body;
  else if (body !== undefined) {
    headers["Content-Type"] = "application/json";
    payload = JSON.stringify(body);
  }
  const ctl = new AbortController();
  const timer = timeoutMs ? setTimeout(() => ctl.abort(), timeoutMs) : null;
  try {
    const res = await fetch(`${base}${path}`, { method, headers, body: payload, signal: ctl.signal });
    const text = await res.text();
    const json = text ? (JSON.parse(text) as unknown) : null;
    if (!res.ok) {
      const err = (json as { error?: string } | null)?.error ?? `${res.status} ${res.statusText}`;
      throw new ApiError(res.status, err);
    }
    return json as T;
  } finally {
    if (timer) clearTimeout(timer);
  }
}

// Same as request, but drives the status bar: busy while in flight, error text on failure.
async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  setStatus("busy", `${method} ${path}`);
  try {
    const r = await request<T>(method, path, body);
    if (app.status.kind === "busy") setStatus("idle");
    return r;
  } catch (e) {
    setStatus("error", e instanceof Error ? e.message : String(e));
    throw e;
  }
}

export const api = {
  health: () => request<Health>("GET", "/health", undefined, 1000),
  discover: (wait?: number) => call<{ cards: Card[] }>("POST", "/discover", { wait }),
  getSettings: () => call<Settings>("GET", "/settings"),
  putSettings: (s: Settings) => call<Settings>("PUT", "/settings", s),
  brightness: (value: number) => call<{ value: number }>("POST", "/brightness", { value }),
  showImageFile: (file: File, fit: Fit, hold: boolean) => {
    const fd = new FormData();
    fd.append("file", file);
    fd.append("fit", fit);
    fd.append("hold", String(hold));
    return call<Outcome | { id: string }>("POST", "/show/image", fd);
  },
  showImagePath: (path: string, fit: Fit, hold: boolean) => call<Outcome | { id: string }>("POST", "/show/image", { path, fit, hold }),
  showVideo: (b: { path: string; loop?: boolean; fps?: number; fit?: Fit; layout?: Canvas }) => call<{ id: string }>("POST", "/show/video", b),
  showPattern: (name: PatternName, hold: boolean) => call<Outcome | { id: string }>("POST", "/show/pattern", { name, hold }),
  showFill: (rgb: string, hold: boolean) => call<Outcome | { id: string }>("POST", "/show/fill", { rgb, hold }),
  showBlank: () => call<Outcome>("POST", "/show/blank"),
  configGen: (spec_toml: string) => call<GenFiles>("POST", "/config/gen", { spec_toml }),
  configRead: (b: { index?: number } = {}) => call<{ rcvbp: string; lines: Line[] }>("POST", "/config/read", b),
  configWrite: (rcvbp: string, commit: boolean) => call<GatedOutcome>("POST", "/config/write", { rcvbp, commit }),
  configSend: (spec_toml: string) => call<Outcome>("POST", "/config/send", { spec_toml }),
  provision: (b: { spec_toml: string; firmware_path?: string; position: [number, number]; commit: boolean }) =>
    call<{ id: string }>("POST", "/provision", b),
  flashSnapshot: (dir?: string) => call<{ id: string }>("POST", "/flash/snapshot", dir ? { dir } : {}),
  flashRestore: (dir: string, commit: boolean) => call<{ id: string }>("POST", "/flash/restore", { dir, commit }),
  firmwareInstall: (path: string, commit: boolean) => call<{ id: string }>("POST", "/firmware/install", { path, commit }),
  getScreenSize: () => call<{ width: number; height: number }>("GET", "/card/screen-size"),
  putScreenSize: (width: number, height: number, commit: boolean) =>
    call<GatedOutcome & { width: number; height: number }>("PUT", "/card/screen-size", { width, height, commit }),
  reload: (full = false) => call<Outcome>("POST", "/card/reload", { full }),
  testMode: (n: number) => call<Outcome>("POST", "/card/test-mode", { n }),
  setLayout: (panel_width: number, panel_height: number) => call<Outcome>("POST", "/card/set-layout", { panel_width, panel_height }),
  getWall: () => call<Canvas>("GET", "/wall"),
  putWall: (c: Canvas) => call<Canvas>("PUT", "/wall", c),
  jobs: () => call<Job[]>("GET", "/jobs"),
  job: (id: string) => call<Job>("GET", `/jobs/${id}`),
  cancel: (id: string) => call<Job>("DELETE", `/jobs/${id}`),
};

// Follow a job's event stream. Returns a function that stops listening.
export function sse(jobId: string, onLine: (l: Line) => void, onEnd: (j: Job) => void): () => void {
  if (MOCK) return mock.sse(jobId, onLine, onEnd);
  const url = `${base}/jobs/${jobId}/events${token ? `?token=${encodeURIComponent(token)}` : ""}`;
  const es = new EventSource(url);
  es.addEventListener("line", (e) => onLine(JSON.parse((e as MessageEvent).data) as Line));
  es.addEventListener("end", (e) => {
    es.close();
    onEnd(JSON.parse((e as MessageEvent).data) as Job);
  });
  es.onerror = () => {
    // The stream closed without `end`: fetch the final state once.
    es.close();
    api.job(jobId).then(onEnd).catch(() => {});
  };
  return () => es.close();
}

// Start following a job in the status bar.
export async function follow(id: string): Promise<Job> {
  const job = await api.job(id);
  app.job = job;
  setStatus("busy", `${job.kind} ${job.id}`);
  return new Promise((resolve) => {
    sse(
      id,
      (l) => {
        if (app.job?.id === id) {
          app.job.lines.push(l);
          setStatus("busy", `${job.kind} ${job.id}: ${l.text}`);
        }
      },
      (j) => {
        if (app.job?.id === id) app.job = j;
        if (j.state === "failed") setStatus("error", `${j.kind} ${j.id}: ${j.error ?? "failed"}`);
        else setStatus("idle", `${j.kind} ${j.id}: ${j.state}`);
        resolve(j);
      },
    );
  });
}

export const isJob = (r: unknown): r is { id: string } => typeof r === "object" && r !== null && "id" in r;

// Probe the daemon once at load; the banner's "retry" calls this again.
export async function probe() {
  app.daemon = "probing";
  try {
    app.health = await api.health();
    app.daemon = "present";
    app.banner = false;
    try {
      const [settings, wall] = await Promise.all([api.getSettings(), api.getWall()]);
      app.settings = settings;
      app.wall = wall;
    } catch {
      /* the status bar already shows the error */
    }
  } catch {
    app.daemon = "absent";
    app.health = null;
    let dismissed = false;
    try {
      dismissed = sessionStorage.getItem("e120.banner") === "off";
    } catch {
      /* no storage */
    }
    app.banner = !dismissed;
  }
  if (!location.hash || location.hash === "#/" || location.hash === "#") {
    location.hash = app.daemon === "present" ? "#/cards" : "#/builder";
  }
}
