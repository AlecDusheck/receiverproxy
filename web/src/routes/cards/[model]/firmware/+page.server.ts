import { error } from "@sveltejs/kit";
import { cards, firmware, imageLocation } from "$lib/server/config";

export const prerender = true;

export const entries = () => cards().map((c) => ({ model: c.name.toLowerCase() }));

export function load({ params }) {
  const card = cards().find((c) => c.name.toLowerCase() === params.model.toLowerCase());
  if (!card) error(404, `${params.model}: no card model of that name`);
  const fw = firmware();
  return {
    card,
    // The images the model's [[tested]] entries name, marked in the table.
    tested: card.tested.map((t) => t.firmware),
    images: fw.images.map((i) => ({ ...i, location: imageLocation(fw, i) })),
  };
}
