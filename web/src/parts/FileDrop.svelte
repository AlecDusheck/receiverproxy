<script lang="ts" module>
  /** What the parent gets: the file, its bytes and their sha256. */
  export type Picked = { file: File; bytes: Uint8Array; sha256: string };
</script>

<script lang="ts">
  // One file, dropped or chosen, with its name, size and sha256 under the
  // target. The bytes are read once and handed to the parent; the hash comes
  // from the browser, which computes it only in a secure context (https or
  // localhost, where the daemon serves).
  import Drop from "./Drop.svelte";
  import { errText } from "$lib/error";

  let {
    label,
    accept = "",
    disabled = false,
    picked = $bindable(null),
    onpick,
  }: { label: string; accept?: string; disabled?: boolean; picked?: Picked | null; onpick?: (p: Picked) => void } = $props();

  let error = $state("");

  async function sha256(bytes: Uint8Array): Promise<string> {
    if (!globalThis.crypto?.subtle) return "sha256 needs https or localhost";
    const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
    return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  }

  async function take(files: File[]) {
    const file = files[0];
    if (!file) return;
    error = "";
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const p = { file, bytes, sha256: await sha256(bytes) };
      picked = p;
      onpick?.(p);
    } catch (e) {
      picked = null;
      error = errText(e);
    }
  }
</script>

<Drop {label} {accept} {disabled} onfiles={(f) => void take(f)} />
{#if picked}
  <dl class="picked">
    <dt>name</dt>
    <dd>{picked.file.name}</dd>
    <dt>bytes</dt>
    <dd>{picked.bytes.length}</dd>
    <dt>sha256</dt>
    <dd>{picked.sha256}</dd>
  </dl>
{/if}
{#if error}<p class="error">{error}</p>{/if}

<style>
  .picked {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--s1) var(--s3);
    margin: calc(-1 * var(--s3)) 0 var(--s4);
  }
  dt {
    color: var(--text-2);
    font-size: 12px;
  }
  dd {
    margin: 0;
    font-family: var(--mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
</style>
