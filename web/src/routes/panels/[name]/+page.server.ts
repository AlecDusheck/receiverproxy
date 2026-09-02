import { error } from "@sveltejs/kit";
import { cards, FORMATS, panels } from "$lib/server/config";

export const prerender = true;

/** One page per spec, whether or not the table links it. */
export const entries = () => panels().map((p) => ({ name: p.name }));

export function load({ params }) {
  const entry = panels().find((p) => p.name === params.name);
  if (!entry) error(404, `${params.name}: no panel spec of that name`);
  // The card models whose [[tested]] entries name this spec, with the firmware each ran.
  const tested = cards().flatMap((c) => c.tested.filter((t) => t.panel === entry.path).map((t) => ({ card: c.name, firmware: t.firmware })));
  return { entry, tested, formats: FORMATS };
}
