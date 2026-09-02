import { error } from "@sveltejs/kit";
import { FORMATS, panels } from "$lib/server/config";

export const prerender = true;

/** One page per spec, whether or not the table links it. */
export const entries = () => panels().map((p) => ({ name: p.name }));

export function load({ params }) {
  const entry = panels().find((p) => p.name === params.name);
  if (!entry) error(404, `${params.name}: no panel spec of that name`);
  return { entry, formats: FORMATS };
}
