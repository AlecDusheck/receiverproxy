// Save bytes or text as a file through a Blob URL.
export function save(name: string, data: Uint8Array | string) {
  const blob = typeof data === "string" ? new Blob([data], { type: "text/plain" }) : new Blob([data as BlobPart], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

export const b64 = (s: string): Uint8Array => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
export const toB64 = (b: Uint8Array): string => btoa(Array.from(b, (c) => String.fromCharCode(c)).join(""));
