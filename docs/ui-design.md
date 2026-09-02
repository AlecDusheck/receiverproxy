# Web app design

The audience is LED integrators and panel makers who spend their days in
LEDVISION, NovaLCT and LEDStudio. They do not want a marketing site or a
dashboard. They want a tool that shows every number, never hides state, works
offline, and behaves the same every time. The reference points are
Wireshark, a good hex editor and a system preferences pane, not a SaaS
landing page.

## Principles

1. Dense and legible. Information first; whitespace only where it separates.
2. Native. System font, native controls, system colours where the platform
   gives them, light and dark from the OS setting, no custom scrollbars.
3. One accent. Everything else is grey. Colour means state (error, busy,
   committed), not decoration.
4. Nothing hidden. Values are shown, not summarised. Hex is hex. Bytes are
   bytes. A disabled action says why in place.
5. No surprise. No modals except a confirm before a write to the card; no
   toasts; results appear where the action was.
6. Text, not icons. An icon needs a label to be understood by this audience;
   the label alone is enough.
7. Keyboard works. Tab order follows reading order; Enter submits; Escape
   cancels; the Builder form is usable without a mouse.
8. Fast. First paint under 100 ms on a static host; the WASM module loads
   lazily; no animation longer than 120 ms and none that conveys information.
9. Every word is a fact. No taglines, no encouragement, no placeholder copy,
   no explanatory paragraphs where a label does; a page with nothing to show
   says so in one sentence.

## Anti-patterns (each one reads as generated)

Hero sections, gradients, drop shadows on cards, rounded 16 px corners,
emoji, icon buttons without labels, skeleton loaders, confetti, toasts,
"Welcome" copy, feature tiles, three-column marketing rows, purple accents,
oversized headings, empty states with illustrations, sentences that end in
an exclamation mark.

## Layout

```
+-------------------------------------------------------------+
| receiverproxy   Panels  Cards  Builder  Wall  Control  GitHub|
+-------------------------------------------------------------+
| banner, only when it applies: daemon not running / token     |
+-------------------------------------------------------------+
|  title row: page name                    [primary action]    |
|  content: tables and forms, up to 960 px, left-aligned       |
+-------------------------------------------------------------+
| footer: version                                              |
+-------------------------------------------------------------+
```

No sidebar. A 44 px top bar with the project name at the left, the pages
as text links, the GitHub link at the right; on narrow screens the links
wrap to a second row, nothing collapses behind a menu. The daemon state is
a one-line banner under the bar: absent (install command, dismiss for the
session) or present without a token (token field). No status bar; job
progress and errors show where the action was. Tables scroll horizontally
inside their own container on narrow screens; forms stack to one column
under 640 px; the wall drawing scales to the viewport width.

## Styling

Tailwind, minimal: the tokens below are the Tailwind theme, utilities are
used for layout and spacing only, and any style that repeats three times
becomes a class in one stylesheet. No plugin, no preflight overrides beyond
the font stack, no arbitrary values.

## Type and spacing

- Font: `system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`; monospace
  `ui-monospace, "SF Mono", Menlo, Consolas, monospace` for hex, bytes, file
  names, TOML, commands, offsets.
- Sizes: 13 px body, 12 px tables and status bar, 15 px screen title, 11 px
  captions. One weight for headings (600), one for body (400). No italics.
- Spacing scale: 4, 8, 12, 16, 24. Row height 28 px in tables, 32 px for
  controls. Grid gap 8 px.
- Line length in prose blocks at most 72 characters.

## Colour

Tokens only; no literal colours in components.

| token | light | dark | use |
|---|---|---|---|
| bg | #ffffff | #1b1b1d | page |
| bg-2 | #f4f4f5 | #242427 | sidebar, table header, inputs |
| line | #d9d9de | #36363b | borders, 1 px |
| text | #1c1c1e | #e8e8ea | body |
| text-2 | #6b6b73 | #9a9aa2 | captions, secondary |
| accent | #2f6fdd | #5b8def | current item, links, primary button |
| ok | #1f8a4c | #3fbf73 | committed, verified |
| warn | #b7791f | #e0a33a | dry run, not verified |
| err | #c0392b | #e5605a | errors, 401, mismatch |

Primary button: accent fill, white text. Every other button: bg-2 fill,
1 px line, text. Destructive or card-writing actions are not red; they are
the same button with the word "commit" and a confirm line above them.

## Components

- Table: header row in bg-2, 1 px lines between rows, right-aligned numbers,
  monospace for hex, sortable by clicking the header, no zebra stripes.
- Form: label above control, 4 px gap, unit or range in the caption
  (`serial_clock 1-31`), invalid value marked with the err token and the
  message under the control, never a popup.
- Key-value block: two columns, keys in text-2, values in monospace, used for
  card info, record fields, job results.
- Bytes: 16 per row, offset column, monospace, differences in the accent.
- Log: the job's lines as they arrive, monospace, newest at the bottom,
  auto-scroll off once the user scrolls up.
- Confirm: one line above the button, `This writes flash block 7. [commit]`;
  no dialog.

## States

Every action has four visible states and they look the same everywhere:
idle (button enabled), busy (button disabled, label unchanged, progress
lines under the action), done (result where the action was), error (message
under the action in err, verbatim from the API).

Daemon absent: card actions are absent, not greyed. The screens that need it
(Cards) show one sentence and the install command in a code block.

## Screens

- Panels (the gallery): a table with a readable title per row (pitch,
  module, scan, chip: "P2.5 128x64 1/16 SM16269S"), status as a word, the
  formats it generates, which cards it is tested with. Filter row above
  (text, vendor, chip, scan, status). Each row is a page: downloads first,
  then the spec as key-value blocks and the TOML, "open in Builder", and
  with a daemon "provision".
- Cards: a table of receiver models (image, vendor, model, status, tested
  panels); each a page with the photo, identity, limits, memory map, tested
  panels and firmware downloads as a table.
- Builder: two panes. Left the form by section (module, screen, chip, colour,
  current, timing, mapping, boot, overrides); right the TOML, editable, kept
  in sync both ways with a 300 ms debounce and the last valid parse shown on
  error. Below: Generate with the output format as a select, then the files
  and the sources list. Import: a file drop target above the form.
- Wall: the layout as a scaled drawing on the left (receivers as outlined
  boxes with their index and position, panels inside; drag moves, snap to
  panel size) and the same data as a table on the right. Import, export,
  layout-example. With a daemon: provision per receiver, show on the wall.
- Cards: a table of discovered cards (index, model, firmware, size, position),
  then the selected card's actions in groups: show, brightness, provision,
  firmware, flash, card. Each group is a form with one button.

## Review checklist

Before a screen is done: no colour outside the tokens; no icon without a
label; every number visible without hovering; every error verbatim; keyboard
path works; dark mode checked; the page has no element wider than 960 px
except the wall drawing; the file is under 300 lines.
