// The firmware manifest as a file, prerendered from config/firmware.toml so
// the client-only Control screens can read it without a daemon: /control
// and /control/provision fetch it once (lib/firmware.ts). The prerendered
// copy is in the site and in the build the daemon embeds.
import { firmware, imageLocation } from "$lib/server/config";

export const prerender = true;

export function GET() {
  const fw = firmware();
  const body = {
    size: fw.size,
    images: fw.images.map((i) => ({ ...i, location: imageLocation(fw, i) })),
  };
  return new Response(JSON.stringify(body), { headers: { "Content-Type": "application/json" } });
}
