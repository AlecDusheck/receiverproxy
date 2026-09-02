import { FORMATS, panels } from "$lib/server/config";

export const prerender = true;

export function load() {
  // The table's rows without the TOML; the entry pages carry it.
  return {
    entries: panels().map(({ toml: _toml, ...e }) => e),
    formats: FORMATS,
  };
}
