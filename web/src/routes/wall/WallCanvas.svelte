<script lang="ts" module>
  export type Sel = { kind: "receiver" | "panel"; i: number } | null;
</script>

<script lang="ts">
  // The layout as a scaled drawing: receivers as outlined boxes with their
  // index and position, panels inside; drag moves, snapped to the panel size.
  import { app } from "$lib/state.svelte";
  import { clamp, place, rotated, snap, snapSize } from "$lib/layout";

  let { sel = $bindable() }: { sel: Sel } = $props();
  let el = $state<HTMLCanvasElement | null>(null);
  // The drawing fits the width it is given (the viewport on a phone) and 400 px of height.
  let avail = $state(800);

  const wall = $derived(app.wall);
  const grid = $derived(snapSize(wall));
  const scale = $derived(Math.max(0.05, Math.min(4, Math.max(64, avail - 2) / Math.max(1, wall.width), 400 / Math.max(1, wall.height))));

  // Colours come from the tokens on the element, so the drawing follows the scheme.
  const token = (css: CSSStyleDeclaration, name: string) => css.getPropertyValue(name).trim();

  $effect(() => {
    const c = el;
    if (!c) return;
    const dpr = devicePixelRatio || 1;
    c.width = Math.ceil(wall.width * scale * dpr) + dpr;
    c.height = Math.ceil(wall.height * scale * dpr) + dpr;
    c.style.width = `${wall.width * scale + 1}px`;
    c.style.height = `${wall.height * scale + 1}px`;
    const g = c.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0.5, 0.5);
    const css = getComputedStyle(c);
    const line = token(css, "--line"), fill = token(css, "--bg-2"), text = token(css, "--text");
    const accent = token(css, "--accent"), accentText = token(css, "--accent-text");
    g.clearRect(-1, -1, c.width, c.height);
    g.font = "11px system-ui, sans-serif";
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
    wall.panels.forEach((p, i) => {
      const [w, h] = rotated(p);
      const x = p.x * scale, y = p.y * scale, W = w * scale, H = h * scale;
      const on = sel?.kind === "panel" && sel.i === i;
      g.fillStyle = on ? accent : fill;
      g.fillRect(x, y, W, H);
      g.strokeStyle = on ? accent : text;
      g.strokeRect(x, y, W, H);
      // arrow: up for none, turned with the panel, mirrored for flips
      g.save();
      g.translate(x + W / 2, y + H / 2);
      g.rotate({ none: 0, cw90: Math.PI / 2, rot180: Math.PI, ccw90: -Math.PI / 2 }[p.rotation ?? "none"]);
      g.scale(p.flip_x ? -1 : 1, p.flip_y ? -1 : 1);
      const a = Math.min(W, H) / 4;
      g.strokeStyle = on ? accentText : text;
      g.beginPath();
      g.moveTo(0, a);
      g.lineTo(0, -a);
      g.moveTo(-a / 2, -a / 2);
      g.lineTo(0, -a);
      g.lineTo(a / 2, -a / 2);
      g.stroke();
      g.restore();
      g.fillStyle = on ? accentText : text;
      g.fillText(`panel ${i} ${p.x},${p.y}`, x + 3, y + H - 3);
    });
    wall.receivers.forEach((r, i) => {
      const on = sel?.kind === "receiver" && sel.i === i;
      const rx = r.x ?? 0, ry = r.y ?? 0;
      g.strokeStyle = on ? accent : line;
      g.lineWidth = on ? 2 : 1;
      g.strokeRect(rx * scale, ry * scale, r.width * scale, r.height * scale);
      g.lineWidth = 1;
      g.fillStyle = on ? accent : text;
      g.fillText(`card ${r.index} ${rx},${ry}`, rx * scale + 3, ry * scale + 12);
    });
  });

  let drag: { sel: Sel; dx: number; dy: number } | null = null;
  const pt = (e: PointerEvent) => {
    const r = el!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  };
  function down(e: PointerEvent) {
    const { x, y } = pt(e);
    let hit: Sel = null;
    for (let i = wall.panels.length - 1; i >= 0 && !hit; i--) {
      const p = wall.panels[i]!;
      const [w, h] = rotated(p);
      if (x >= p.x && x < p.x + w && y >= p.y && y < p.y + h) hit = { kind: "panel", i };
    }
    for (let i = wall.receivers.length - 1; i >= 0 && !hit; i--) {
      const r = wall.receivers[i]!;
      if (x >= (r.x ?? 0) && x < (r.x ?? 0) + r.width && y >= (r.y ?? 0) && y < (r.y ?? 0) + r.height) hit = { kind: "receiver", i };
    }
    sel = hit;
    if (!hit) return;
    const o = hit.kind === "panel" ? wall.panels[hit.i]! : wall.receivers[hit.i]!;
    drag = { sel: hit, dx: x - (o.x ?? 0), dy: y - (o.y ?? 0) };
    el!.setPointerCapture(e.pointerId);
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
    el?.releasePointerCapture(e.pointerId);
  }
</script>

<div class="frame" bind:clientWidth={avail}>
  <canvas bind:this={el} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up} aria-label="wall drawing"></canvas>
</div>

<style>
  .frame {
    width: 100%;
    min-width: 0;
  }
  canvas {
    display: block;
    touch-action: none;
    border: 1px solid var(--line);
    margin-bottom: var(--s2);
  }
</style>
