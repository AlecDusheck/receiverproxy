import { SITE } from "$lib/site";

export const prerender = true;

export function GET() {
  return new Response(`User-agent: *\nAllow: /\nSitemap: ${SITE}/sitemap.xml\n`, { headers: { "Content-Type": "text/plain" } });
}
