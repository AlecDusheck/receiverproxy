// The transport to the daemon's JSON API (docs/ui.md section 2): the base
// URL, the token, one request helper and one SSE helper. Only ops.ts calls
// this. Nothing runs at import time: the layout calls `ops.probe()` once on
// the client, which reads the token first.
import type { Job, Line, State } from "./types";
import { loadToken, setToken } from "../lib/token";

export const base: string = (import.meta.env.VITE_RXP_API as string | undefined) ?? "/api/v1";

// The daemon's token: from the fragment or the tab's storage at load, or typed into the field under the title row.
let token: string | null = null;

export const hasToken = () => token !== null;

/** Read the token from `#token=` or the tab's storage; `replace` rewrites the address bar. */
export function readToken(replace: (url: string) => void) {
  token = loadToken(replace);
}

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
  const headers: Record<string, string> = {};
  if (token) headers["X-Token"] = token;
  let payload: BodyInit | undefined;
  if (body instanceof FormData) payload = body;
  else if (body instanceof Uint8Array) {
    headers["Content-Type"] = "application/octet-stream";
    payload = body as BodyInit;
  } else if (body !== undefined) {
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

// A request with no timeout: the daemon answers a card command when the card does.
export const call = <T>(method: string, path: string, body?: unknown): Promise<T> => request<T>(method, path, body);

const stream = (path: string) => new EventSource(`${base}${path}${token ? `${path.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}` : ""}`);

// Follow the daemon's state: one message at once and one on every change.
// Returns a function that stops listening.
export function stateSse(onState: (s: State) => void, onError: () => void): () => void {
  const es = stream("/state/events");
  es.addEventListener("state", (e) => onState(JSON.parse((e as MessageEvent).data) as State));
  es.onerror = () => {
    es.close();
    onError();
  };
  return () => es.close();
}

// Follow a job's event stream. Returns a function that stops listening.
export function sse(jobId: string, onLine: (l: Line) => void, onEnd: (j: Job) => void): () => void {
  const es = stream(`/jobs/${jobId}/events`);
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
