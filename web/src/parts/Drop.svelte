<script lang="ts">
  // A file drop target with a native file input beside its label.
  let { label, accept = "", disabled = false, multiple = false, onfiles }: { label: string; accept?: string; disabled?: boolean; multiple?: boolean; onfiles: (files: File[]) => void } = $props();
  let over = $state(false);

  function take(list: FileList | null | undefined) {
    const files = Array.from(list ?? []);
    if (files.length) onfiles(files);
  }
  function drop(e: DragEvent) {
    e.preventDefault();
    over = false;
    if (!disabled) take(e.dataTransfer?.files);
  }
</script>

<div
  class={["drop", { over }]}
  role="group"
  aria-label={label}
  ondragover={(e) => {
    e.preventDefault();
    over = true;
  }}
  ondragleave={() => (over = false)}
  ondrop={drop}
>
  <span>{label}</span>
  <input type="file" {accept} {disabled} {multiple} onchange={(e) => take(e.currentTarget.files)} />
  <span class="caption">or drop the file here</span>
</div>
