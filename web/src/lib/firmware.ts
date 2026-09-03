// The firmware manifest in the browser: the prerendered /firmware.json,
// fetched once per session. The Control screens need it without a daemon
// (the images are named in config/firmware.toml, not held by the card).

export type Image = {
  name: string;
  version: string;
  kind: string;
  pcb?: string;
  chips: string[];
  sha256: string;
  location: { href: string; remote: boolean };
};

export type Manifest = { size: number; images: Image[] };

let pending: Promise<Manifest> | null = null;

/** The manifest; the same promise for every caller. */
export function manifest(): Promise<Manifest> {
  pending ??= fetch("/firmware.json").then((r) => {
    if (!r.ok) throw new Error(`/firmware.json: ${r.status} ${r.statusText}`);
    return r.json() as Promise<Manifest>;
  });
  return pending;
}
