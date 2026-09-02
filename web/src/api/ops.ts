// Every operation the app performs, behind one interface. Pure operations
// run in the browser through the WASM module; card operations go through
// the daemon and exist only while it answers (`ops.card` is null otherwise,
// and the screens hide what needs it). Screens import nothing else from
// api/ or lib/wasm.ts.
import type * as T from "./types";
import { app } from "../lib/state.svelte";
import { call, hasToken, readToken, request, sse, useToken } from "./daemon";
import { current, ready } from "../lib/wasm";
import { example, validateJs } from "../lib/layout";

export type { Entry, Format, GenFile, Generated, Imported, Meta } from "./types";

/** Operations that need no daemon: `rcvbp` and `wall` in the browser. */
export type PureOps = {
  /** Load the module now; every other function does so on first use. */
  load(): Promise<void>;
  /** The panel specs the module embeds, with their metadata, for the Gallery. */
  gallery(): Promise<T.Entry[]>;
  /** The output and input formats the module knows. */
  formats(): Promise<T.Format[]>;
  /** The files for one spec in one output format; `rcvbp` by default. */
  generate(specToml: string, format?: string): Promise<T.Generated>;
  /** A vendor file as a spec; the format is detected from the bytes when not given. */
  importSpec(bytes: Uint8Array, format?: string): Promise<T.Imported>;
  inspect(rcvbp: Uint8Array): Promise<T.Inspection>;
  diff(a: Uint8Array, b: Uint8Array): Promise<T.Diff>;
  libraries(): Promise<T.Libraries>;
  /** "ok" or the `LayoutError` text; the JS check stands in until the module is loaded. */
  validateLayout(c: T.Canvas): string;
  /** `Canvas::cards`: one receiver per panel. */
  layoutExample(cols: number, rows: number, w: number, h: number): T.Canvas;
};

/** Operations that reach the card through the daemon. */
export type CardOps = {
  discover(wait?: number): Promise<T.Card[]>;
  settings(): Promise<T.Settings>;
  saveSettings(s: T.Settings): Promise<T.Settings>;
  brightness(value: number): Promise<number>;
  showImageFile(file: File, fit: T.Fit, hold: boolean): Promise<T.Outcome | T.Started>;
  showImage(req: T.ShowImageReq): Promise<T.Outcome | T.Started>;
  showVideo(req: T.ShowVideoReq): Promise<T.Started>;
  showPattern(req: T.ShowPatternReq): Promise<T.Outcome | T.Started>;
  showFill(req: T.ShowFillReq): Promise<T.Outcome | T.Started>;
  showBlank(): Promise<T.Outcome>;
  configGen(specToml: string): Promise<T.GenFiles>;
  configRead(req?: T.ConfigReadReq): Promise<T.ConfigRead>;
  configWrite(req: T.ConfigWriteReq): Promise<T.GatedOutcome>;
  configSend(req: T.ConfigSendReq): Promise<T.Outcome>;
  provision(req: T.ProvisionReq): Promise<T.Started>;
  flashSnapshot(req?: T.SnapshotReq): Promise<T.Started>;
  flashRestore(req: T.RestoreReq): Promise<T.Started>;
  firmwareInstall(req: T.FirmwareReq): Promise<T.Started>;
  screenSize(q?: T.ScreenSizeQuery): Promise<T.Size>;
  setScreenSize(req: T.ScreenSizeReq): Promise<T.SizeOutcome>;
  reload(req?: T.ReloadReq): Promise<T.Outcome>;
  testMode(req: T.TestModeReq): Promise<T.Outcome>;
  setLayout(req: T.SetLayoutReq): Promise<T.Outcome>;
  wall(): Promise<T.Canvas>;
  saveWall(c: T.Canvas): Promise<T.Canvas>;
  jobs(): Promise<T.Job[]>;
  job(id: string): Promise<T.Job>;
  cancel(id: string): Promise<T.Job>;
  /** Keep `app.job` current over SSE until the job ends; resolves with its final state. */
  follow(id: string): Promise<T.Job>;
};

export type Ops = {
  pure: PureOps;
  /** The card operations while the daemon is present, else null. */
  readonly card: CardOps | null;
  /** Once, on the client: read the token from the address bar, then probe. `replace` rewrites the address bar without it. */
  start(replace: (url: string) => void): Promise<void>;
  /** Ask the daemon whether it is there; sets `app.daemon`. */
  probe(): Promise<void>;
  /** Use a token typed into the banner, then probe again. */
  connect(token: string): Promise<void>;
};

const pure: PureOps = {
  load: async () => {
    await ready();
  },
  gallery: async () => (await ready()).gallery(),
  formats: async () => (await ready()).formats(),
  generate: async (toml, format = "rcvbp") => (await ready()).generate(toml, format),
  importSpec: async (bytes, format) => (await ready()).import(bytes, format),
  inspect: async (bytes) => (await ready()).inspect(bytes),
  diff: async (a, b) => (await ready()).diff(a, b),
  libraries: async () => (await ready()).libraries(),
  validateLayout: (c) => {
    const m = current();
    return m ? m.validate_layout(JSON.stringify(c)) : validateJs(c);
  },
  layoutExample: (cols, rows, w, h) => {
    const m = current();
    return m ? (JSON.parse(m.layout_example(cols, rows, w, h)) as T.Canvas) : example(cols, rows, w, h);
  },
};

const qs = (q: object) => {
  const s = new URLSearchParams(Object.entries(q).filter(([, v]) => v !== undefined) as [string, string][]).toString();
  return s ? `?${s}` : "";
};

async function follow(id: string): Promise<T.Job> {
  const job = await card.job(id);
  app.job = job;
  return new Promise((resolve) => {
    sse(
      id,
      (l) => {
        if (app.job?.id === id) app.job.lines.push(l);
      },
      (j) => {
        if (app.job?.id === id) app.job = j;
        resolve(j);
      },
    );
  });
}

const card: CardOps = {
  discover: async (wait) => (await call<T.Cards>("POST", "/discover", { wait } satisfies T.DiscoverReq)).cards,
  settings: () => call<T.Settings>("GET", "/settings"),
  saveSettings: (s) => call<T.Settings>("PUT", "/settings", s),
  brightness: async (value) => (await call<T.Brightness>("POST", "/brightness", { value } satisfies T.Brightness)).value,
  showImageFile: (file, fit, hold) => {
    const fd = new FormData();
    fd.append("file", file);
    fd.append("fit", fit);
    fd.append("hold", String(hold));
    return call("POST", "/show/image", fd);
  },
  showImage: (req) => call("POST", "/show/image", req),
  showVideo: (req) => call("POST", "/show/video", req),
  showPattern: (req) => call("POST", "/show/pattern", req),
  showFill: (req) => call("POST", "/show/fill", req),
  showBlank: () => call("POST", "/show/blank"),
  configGen: (spec_toml) => call("POST", "/config/gen", { spec_toml } satisfies T.SpecReq),
  configRead: (req = {}) => call("POST", "/config/read", req),
  configWrite: (req) => call("POST", "/config/write", req),
  configSend: (req) => call("POST", "/config/send", req),
  provision: (req) => call("POST", "/provision", req),
  flashSnapshot: (req = {}) => call("POST", "/flash/snapshot", req),
  flashRestore: (req) => call("POST", "/flash/restore", req),
  firmwareInstall: (req) => call("POST", "/firmware/install", req),
  screenSize: (q = {}) => call("GET", `/card/screen-size${qs(q)}`),
  setScreenSize: (req) => call("PUT", "/card/screen-size", req),
  reload: (req = {}) => call("POST", "/card/reload", req),
  testMode: (req) => call("POST", "/card/test-mode", req),
  setLayout: (req) => call("POST", "/card/set-layout", req),
  wall: () => call("GET", "/wall"),
  saveWall: (c) => call("PUT", "/wall", c),
  jobs: () => call("GET", "/jobs"),
  job: (id) => call("GET", `/jobs/${id}`),
  cancel: (id) => call("DELETE", `/jobs/${id}`),
  follow,
};

// Probe the daemon once at load; the banner's "retry" and "connect" call this again.
// "locked": the daemon answered but the app has no token, or a wrong one.
async function probe() {
  app.daemon = "probing";
  app.tokenError = "";
  try {
    // Without the token the body is `{ version }` alone.
    const h = await request<T.Health>("GET", "/health", undefined, 1000);
    if (h.iface === undefined || h.cards === undefined) {
      app.daemon = "locked";
      app.health = null;
      if (hasToken()) app.tokenError = "bad token";
    } else {
      app.health = { version: h.version, iface: h.iface, cards: h.cards };
      app.daemon = "present";
      app.install = false;
      try {
        const [settings, wall] = await Promise.all([card.settings(), card.wall()]);
        app.settings = settings;
        app.wall = wall;
      } catch {
        /* the screens that need settings or the wall ask again */
      }
    }
  } catch {
    app.daemon = "absent";
    app.health = null;
  }
  if (app.daemon === "absent") {
    let dismissed = false;
    try {
      dismissed = sessionStorage.getItem("rxp.install") === "off";
    } catch {
      /* no storage */
    }
    app.install = !dismissed;
  }
}

export const ops: Ops = {
  pure,
  get card() {
    return app.daemon === "present" ? card : null;
  },
  start(replace) {
    readToken(replace);
    return probe();
  },
  probe,
  connect(token) {
    useToken(token);
    return probe();
  },
};
