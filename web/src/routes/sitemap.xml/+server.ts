// Every prerendered page, lastmod the build date; the client-only routes
// (builder, wall, control) are noindex and absent.
import { cards, panels } from "$lib/server/config";
import { SITE } from "$lib/site";

export const prerender = true;

export function GET() {
  const card = (name: string) => `/cards/${encodeURIComponent(name.toLowerCase())}`;
  const paths = [
    "/",
    "/panels",
    ...panels().map((p) => `/panels/${encodeURIComponent(p.name)}`),
    "/cards",
    ...cards().flatMap((c) => [card(c.name), `${card(c.name)}/firmware`]),
  ];
  const lastmod = new Date().toISOString().slice(0, 10);
  const body = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${paths.map((p) => `  <url><loc>${SITE}${p}</loc><lastmod>${lastmod}</lastmod></url>`).join("\n")}\n</urlset>\n`;
  return new Response(body, { headers: { "Content-Type": "application/xml" } });
}
