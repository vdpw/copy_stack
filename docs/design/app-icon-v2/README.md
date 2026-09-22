# Copy Stack — Blue Stack Icon

New app-icon design based on the current UI theme in `src/macos.css` and
`src/features/settings/settings.css`.

The three offset clipboard cards express copied items collected into a stack.
A pearl-white rounded tile, a solid blue foreground card, and restrained glass
edges echo the application's grouped surfaces, system-blue controls, and light
navigation translucency.

Palette references: `#0068df`, `#1870d8`, `#f3f3f5`, and `#ecebea`.
Generated image colors are visual interpretations of those tokens.

Created with the built-in image generation tool. The complete generation prompt
is retained in `prompt.txt`; `refinement-prompt.txt` records the edge-cleanup pass.

## Assets

- `copy-stack-icon.png`: 1254 × 1254 PNG with transparency.
- `previews/icon-128.png`: 128 × 128 size preview.
- `previews/icon-32.png`: 32 × 32 size preview.

The size previews were exported with macOS `sips`. Both were visually inspected
for the clipboard silhouette, stack separation, and blue/white contrast. No
application code changed, so frontend and Rust checks are not applicable.

This design is saved separately from the currently bundled application icons.
The menu bar uses a separate monochrome template asset.
