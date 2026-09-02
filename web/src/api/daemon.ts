// The transport to the daemon's JSON API (docs/ui.md section 2): the base
// URL, the token, one request helper and one SSE helper. Only ops.ts calls
// this.
import { app, setStatus } from "../lib/state.svelte";
import type { Job, Line } from "./types";
import * as mock from "./mock";
import { loadToken, setToken } from "../lib/token";

const MOCK = import.meta.env.VITE_E120_MOCK === "1";
export const base: string = MOCK ? "mock" : ((import.meta.env.VITE_E120_API as string | undefined) ?? "/api/v1");

// The daemon's token: from the fragment or the tab's storage at load, or typed into the field under the title row.
let token: string | null = loadToken();

export const hasToken = () => token !== null;

export function useToken(t: string) {
  token = t || null;
  setToken(t);
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function request<T>(method: string, path: string, body?: unknown, timeoutMs?: number): Promise<T> {
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
export async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
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
    request<Job>("GET", `/jobs/${jobId}`).then(onEnd).catch(() => {});
  };
  return () => es.close();
}
