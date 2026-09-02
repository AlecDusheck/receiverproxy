// The text of a thrown value: the API's or WASM's message verbatim.
export const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));
