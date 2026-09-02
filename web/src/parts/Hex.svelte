<script lang="ts">
  // A byte slice as a hex dump, 16 per line.
  let { bytes, offset = 0 }: { bytes: Uint8Array; offset?: number } = $props();
  const lines = $derived.by(() => {
    const out: string[] = [];
    for (let i = 0; i < bytes.length; i += 16) {
      const chunk = Array.from(bytes.subarray(i, i + 16), (b) => b.toString(16).padStart(2, "0")).join(" ");
      out.push(`${(offset + i).toString(16).padStart(4, "0")}  ${chunk}`);
    }
    return out.join("\n");
  });
</script>

<pre>{lines}</pre>
