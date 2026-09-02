// The old address of a spec's page: /gallery/<name> is /panels/<name>.
import { redirect } from "@sveltejs/kit";
import { panels } from "$lib/server/config";

export const prerender = true;

export const entries = () => panels().map((p) => ({ name: p.name }));

export function load({ params }) {
  redirect(301, `/panels/${encodeURIComponent(params.name)}`);
}
