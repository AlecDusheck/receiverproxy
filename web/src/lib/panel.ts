// A panel entry's readable title: pitch when [meta] has one, module, scan,
// chip. "P2.5 128x64 1/16 SM16269S".
import type { Entry } from "../api/types";

/** The chip's part number: the library name up to its first space or bracket. */
export const chipLabel = (name: string): string => name.split(/[\s(]/, 1)[0] || name;

export function panelTitle(e: Pick<Entry, "meta" | "module" | "chip">): string {
  const pitch = e.meta.pitch_mm !== undefined ? `P${e.meta.pitch_mm} ` : "";
  return `${pitch}${e.module.width}x${e.module.height} 1/${e.module.scan} ${chipLabel(e.chip.name)}`;
}
