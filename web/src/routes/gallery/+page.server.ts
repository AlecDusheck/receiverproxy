// The old address: /gallery is /panels. Prerendered as a redirect (the
// Cloudflare adapter writes it to _redirects, the static build as a page
// that refreshes to the new one).
import { redirect } from "@sveltejs/kit";

export const prerender = true;

export function load() {
  redirect(301, "/panels");
}
