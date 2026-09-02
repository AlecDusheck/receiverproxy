import { cards, FORMATS, panels } from "$lib/server/config";

export const prerender = true;

export function load() {
  // The table's rows without the TOML; the entry pages carry it. `cards` is
  // the models whose [[tested]] entries name the spec's path.
  const models = cards();
  return {
    entries: panels().map(({ toml: _toml, ...e }) => ({ ...e, cards: models.filter((c) => c.tested.some((t) => t.panel === e.path)).map((c) => c.name) })),
    formats: FORMATS,
  };
}
