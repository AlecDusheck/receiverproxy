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
    // Each tested entry with the spec's name for the gallery link and the firmware version from the manifest.
    tested: card.tested.map((t) => ({
      ...t,
      name: specs.find((p) => p.path === t.panel)?.name ?? null,
      version: fw.images.find((i) => i.name === t.firmware)?.version ?? null,
    })),
    base_url: fw.base_url,
    images: fw.images.map((i) => ({ ...i, location: imageLocation(fw, i) })),
  };
}
