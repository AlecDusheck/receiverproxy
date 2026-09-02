import { cards, firmware } from "$lib/server/config";

export const prerender = true;

export function load() {
  return { cards: cards(), images: firmware().images.length };
}
