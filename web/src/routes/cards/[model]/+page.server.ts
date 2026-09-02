import { error } from "@sveltejs/kit";
import { cards, firmware, imageLocation, panels } from "$lib/server/config";

export const prerender = true;

export const entries = () => cards().map((c) => ({ model: c.name.toLowerCase() }));

export function load({ params }) {
  const card = cards().find((c) => c.name.toLowerCase() === params.model.toLowerCase());
  if (!card) error(404, `${params.model}: no card model of that name`);
  const fw = firmware();
  const specs = panels();
  return {
    card,
    // Each tested entry with the spec (for its title and link) and the firmware version from the manifest.
    tested: card.tested.map((t) => {
      const p = specs.find((s) => s.path === t.panel);
      return { ...t, entry: p ? { name: p.name, meta: p.meta, module: p.module, chip: p.chip } : null, version: fw.images.find((i) => i.name === t.firmware)?.version ?? null };
    }),
    images: fw.images.map((i) => ({ ...i, location: imageLocation(fw, i) })),
  };
}
