import { cards, panels } from "$lib/server/config";
import { SITE } from "$lib/site";

export const prerender = true;

export function GET() {
  const paths = ["/", "/gallery", ...panels().map((p) => `/gallery/${encodeURIComponent(p.name)}`), "/cards", ...cards().map((c) => `/cards/${encodeURIComponent(c.name)}`), "/builder", "/wall"];
  const body = `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${paths.map((p) => `  <url><loc>${SITE}${p}</loc></url>`).join("\n")}\n</urlset>\n`;
  return new Response(body, { headers: { "Content-Type": "application/xml" } });
}
