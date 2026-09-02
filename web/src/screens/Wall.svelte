<script lang="ts">
  import Field from "../parts/Field.svelte";
  import { app } from "../lib/state.svelte";
  import { api } from "../lib/api";
  import { save } from "../lib/download";
  import { ROTATIONS, addPanel, addReceiver, clamp, example, normalize, place, rotated, snap, snapSize, validate } from "../lib/layout";
  import { errText } from "../lib/wasm";
  import type { Canvas } from "../lib/types";

  type Sel = { kind: "receiver" | "panel"; i: number } | null;
  let sel = $state<Sel>(null);
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let importError = $state("");
  let saveError = $state("");
  let saved = $state("");
  let ex = $state({ cols: 2, rows: 1, w: 128, h: 64 });

  const wall = $derived(app.wall);
  const grid = $derived(snapSize(wall));
  const verdict = $derived.by(() => {
    try {
      return app.wasm === "loading" ? "ok" : validate(wall);
    } catch (e) {
      return errText(e);
    }
  });
  const scale = $derived(Math.max(0.5, Math.min(4, 800 / Math.max(1, wall.width), 400 / Math.max(1, wall.height))));

  $effect(() => {
    try {
      localStorage.setItem("e120.wall", JSON.stringify(wall));
    } catch {
      /* no storage */
    }
  });

  // Draw
  $effect(() => {
    const el = canvasEl;
    if (!el) return;
    const dpr = devicePixelRatio || 1;
    el.width = Math.ceil(wall.width * scale * dpr) + dpr;
    el.height = Math.ceil(wall.height * scale * dpr) + dpr;
    el.style.width = `${wall.width * scale + 1}px`;
    el.style.height = `${wall.height * scale + 1}px`;
    const g = el.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0.5, 0.5);
    const css = getComputedStyle(el);
    const line = css.getPropertyValue("--line").trim() || "#ccc";
    const muted = css.getPropertyValue("--muted").trim() || "#eee";
    const text = css.color;
    g.clearRect(-1, -1, el.width, el.height);
    g.font = "11px system-ui, sans-serif";
    // grid
    g.strokeStyle = line;
    g.globalAlpha = 0.5;
    g.beginPath();
    for (let x = 0; x <= wall.width; x += grid) {
      g.moveTo(x * scale, 0);
      g.lineTo(x * scale, wall.height * scale);
    }
    for (let y = 0; y <= wall.height; y += grid) {
      g.moveTo(0, y * scale);
      g.lineTo(wall.width * scale, y * scale);
    }
    g.stroke();
    g.globalAlpha = 1;
    // panels
    wall.panels.forEach((p, i) => {
      const [w, h] = rotated(p);
      const x = p.x * scale, y = p.y * scale, W = w * scale, H = h * scale;
      const on = sel?.kind === "panel" && sel.i === i;
      g.fillStyle = on ? "AccentColor" : muted;
      g.fillRect(x, y, W, H);
      g.strokeStyle = on ? "AccentColor" : text;
      g.strokeRect(x, y, W, H);
      // arrow: up for none, rotated with the panel, mirrored for flips
      g.save();
      g.translate(x + W / 2, y + H / 2);
      const rot = { none: 0, cw90: Math.PI / 2, rot180: Math.PI, ccw90: -Math.PI / 2 }[p.rotation ?? "none"];
      g.rotate(rot);
      g.scale(p.flip_x ? -1 : 1, p.flip_y ? -1 : 1);
      const a = Math.min(W, H) / 4;
      g.strokeStyle = on ? "AccentColorText" : text;
      g.beginPath();
      g.moveTo(0, a);
      g.lineTo(0, -a);
      g.moveTo(-a / 2, -a / 2);
      g.lineTo(0, -a);
      g.lineTo(a / 2, -a / 2);
      g.stroke();
      g.restore();
      g.fillStyle = on ? "AccentColorText" : text;
      g.fillText(`${i}`, x + 3, y + H - 3);
    });
    // receivers
    wall.receivers.forEach((r, i) => {
      const on = sel?.kind === "receiver" && sel.i === i;
      g.strokeStyle = on ? "AccentColor" : line;
      g.lineWidth = on ? 2 : 1;
      g.strokeRect((r.x ?? 0) * scale, (r.y ?? 0) * scale, r.width * scale, r.height * scale);
      g.lineWidth = 1;
      g.fillStyle = on ? "AccentColor" : text;
      g.fillText(`card ${r.index}`, (r.x ?? 0) * scale + 3, (r.y ?? 0) * scale + 12);
    });
  });

  // Drag
  let drag: { sel: Sel; dx: number; dy: number } | null = null;
  const pt = (e: PointerEvent) => {
    const r = canvasEl!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  };
  function down(e: PointerEvent) {
    const { x, y } = pt(e);
    let hit: Sel = null;
    for (let i = wall.panels.length - 1; i >= 0; i--) {
      const p = wall.panels[i]!;
      const [w, h] = rotated(p);
      if (x >= p.x && x < p.x + w && y >= p.y && y < p.y + h) {
        hit = { kind: "panel", i };
        break;
      }
    }
    if (!hit)
      for (let i = wall.receivers.length - 1; i >= 0; i--) {
        const r = wall.receivers[i]!;
        if (x >= (r.x ?? 0) && x < (r.x ?? 0) + r.width && y >= (r.y ?? 0) && y < (r.y ?? 0) + r.height) {
          hit = { kind: "receiver", i };
          break;
        }
      }
    sel = hit;
    if (!hit) return;
    const o = hit.kind === "panel" ? wall.panels[hit.i]! : wall.receivers[hit.i]!;
    drag = { sel: hit, dx: x - (o.x ?? 0), dy: y - (o.y ?? 0) };
    canvasEl!.setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag?.sel) return;
    const { x, y } = pt(e);
    const nx = snap(x - drag.dx, grid), ny = snap(y - drag.dy, grid);
    if (drag.sel.kind === "panel") {
      const p = wall.panels[drag.sel.i]!;
      const [w, h] = rotated(p);
      place(wall, p, clamp(nx, 0, wall.width - w), clamp(ny, 0, wall.height - h));
    } else {
      const r = wall.receivers[drag.sel.i]!;
      const ox = r.x ?? 0, oy = r.y ?? 0;
      r.x = clamp(nx, 0, wall.width - r.width);
      r.y = clamp(ny, 0, wall.height - r.height);
      for (const p of wall.panels) if (p.receiver === r.index) place(wall, p, p.x + r.x - ox, p.y + r.y - oy);
    }
  }
  function up(e: PointerEvent) {
    drag = null;
    canvasEl?.releasePointerCapture(e.pointerId);
  }

  function remove() {
    if (!sel) return;
    if (sel.kind === "panel") wall.panels.splice(sel.i, 1);
    else {
      const idx = wall.receivers[sel.i]!.index;
      wall.receivers.splice(sel.i, 1);
      app.wall.panels = wall.panels.filter((p) => p.receiver !== idx);
    }
    sel = null;
  }
  function setWall(c: Canvas) {
    app.wall = normalize(c);
    sel = null;
  }
  async function importFile(files: FileList | null) {
    const f = files?.[0];
    if (!f) return;
    importError = "";
    try {
      setWall(JSON.parse(await f.text()) as Canvas);
    } catch (e) {
      importError = errText(e);
    }
  }
  async function saveDaemon() {
    saveError = "";
    saved = "";
    try {
      app.wall = await api.putWall(normalize(wall));
      saved = "saved as the daemon's wall";
    } catch (e) {
      saveError = errText(e);
    }
  }
  const busy = $derived(app.status.kind === "busy");
</script>

<h1>Wall</h1>
<p class="muted">The layout <code>e120 show --layout</code> reads: receivers are cards, each keeping its window of the screen; panels hang off a receiver. Drag to move; positions snap to {grid} px.</p>

<div class="row" style="margin-bottom: var(--s3)">
  <label>screen <input type="number" bind:value={app.wall.width} min="1" /> x <input type="number" bind:value={app.wall.height} min="1" /></label>
  <button onclick={() => { addReceiver(wall); sel = { kind: "receiver", i: wall.receivers.length - 1 }; }}>add card</button>
  <button onclick={() => { const r = sel?.kind === "receiver" ? wall.receivers[sel.i]!.index : (wall.receivers[0]?.index ?? 0); addPanel(wall, r); sel = { kind: "panel", i: wall.panels.length - 1 }; }} disabled={!wall.receivers.length}>add panel</button>
  <button onclick={remove} disabled={!sel}>remove selected</button>
  <span class="sep"></span>
  <label>example <input type="number" bind:value={ex.cols} min="1" style="width: 56px" /> x <input type="number" bind:value={ex.rows} min="1" style="width: 56px" /> cards of <input type="number" bind:value={ex.w} min="1" /> x <input type="number" bind:value={ex.h} min="1" /></label>
  <button onclick={() => setWall(example(ex.cols, ex.rows, ex.w, ex.h))}>generate</button>
</div>

<canvas bind:this={canvasEl} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}></canvas>
<p class={verdict === "ok" ? "ok" : "error"}>{verdict}</p>

<section>
  <h2>Receivers</h2>
  <div class="scroll">
    <table>
      <thead><tr><th>index</th><th>x</th><th>y</th><th>width</th><th>height</th><th></th></tr></thead>
      <tbody>
        {#each wall.receivers as r, i (i)}
          <tr class={{ selected: sel?.kind === "receiver" && sel.i === i }} onclick={() => (sel = { kind: "receiver", i })}>
            <td><input type="number" bind:value={r.index} min="0" /></td>
            <td><input type="number" bind:value={r.x} min="0" step={grid} /></td>
            <td><input type="number" bind:value={r.y} min="0" step={grid} /></td>
            <td><input type="number" bind:value={r.width} min="1" /></td>
            <td><input type="number" bind:value={r.height} min="1" /></td>
            <td>
              {#if app.daemon === "present"}
                <a href="#/cards?provision={r.index}">provision this card</a>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<section>
  <h2>Panels</h2>
  <div class="scroll">
    <table>
      <thead><tr><th>#</th><th>receiver</th><th>receiver_x</th><th>receiver_y</th><th>x</th><th>y</th><th>width</th><th>height</th><th>rotation</th><th>flip_x</th><th>flip_y</th></tr></thead>
      <tbody>
        {#each wall.panels as p, i (i)}
          <tr class={{ selected: sel?.kind === "panel" && sel.i === i }} onclick={() => (sel = { kind: "panel", i })}>
            <td>{i}</td>
            <td>
              <select bind:value={p.receiver}>
                {#each wall.receivers as r (r.index)}<option value={r.index}>{r.index}</option>{/each}
              </select>
            </td>
            <td><input type="number" bind:value={p.receiver_x} min="0" step={grid} /></td>
            <td><input type="number" bind:value={p.receiver_y} min="0" step={grid} /></td>
            <td><input type="number" bind:value={p.x} min="0" step={grid} /></td>
            <td><input type="number" bind:value={p.y} min="0" step={grid} /></td>
            <td><input type="number" bind:value={p.width} min="1" /></td>
            <td><input type="number" bind:value={p.height} min="1" /></td>
            <td>
              <select bind:value={p.rotation}>
                {#each ROTATIONS as r (r)}<option value={r}>{r}</option>{/each}
              </select>
            </td>
            <td><input type="checkbox" bind:checked={p.flip_x} /></td>
            <td><input type="checkbox" bind:checked={p.flip_y} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<section>
  <h2>File</h2>
  <div class="form">
    <Field label="import JSON" error={importError}><input type="file" accept=".json,application/json" onchange={(e) => importFile(e.currentTarget.files)} /></Field>
    <Field label="export">
      <button onclick={() => save("wall.json", JSON.stringify(normalize(wall), null, 2) + "\n")}>download wall.json</button>
      {#if app.daemon === "present"}
        <button class="primary" onclick={saveDaemon} disabled={busy || verdict !== "ok"}>save as the daemon's wall</button>
        {#if saved}<span class="ok">{saved}</span>{/if}
      {/if}
    </Field>
    {#if saveError}<div class="wide error">{saveError}</div>{/if}
  </div>
</section>

<style>
  canvas {
    display: block;
    touch-action: none;
    max-width: 100%;
    margin-bottom: var(--s2);
  }
  .sep {
    width: var(--s4);
  }
  tr.selected td {
    background: var(--muted);
  }
</style>
