# CTX Design System

## Scene and strategy

An early-morning research desk: a white writing surface, a cool graphite navigation rail, and one restrained cobalt action color. The interface is a document tool, not a marketing page. It assigns visual emphasis to the document, its publication state, and the public reading experience instead of operational statistics.

## Color tokens

```css
:root {
  --background: oklch(1 0 0);
  --surface: oklch(0.976 0.006 250);
  --foreground: oklch(0.21 0.016 250);
  --muted-foreground: oklch(0.45 0.018 250);
  --primary: oklch(0.49 0.149 250);
  --primary-foreground: oklch(0.99 0 0);
  --accent: oklch(0.30 0.055 185);
  --destructive: oklch(0.53 0.19 28);
  --border: oklch(0.90 0.010 250);
}
```

The strategy is restrained. Cobalt is reserved for the selected location, the primary action, keyboard focus, and intentional AI work. Semantic badges communicate publication and task state with an icon and text label.

## Typography

Geist is the UI face. The editing canvas uses a measured 16px body and a 72ch prose line length; the application shell uses compact 14px controls and 24px page headings. Markdown and source metadata use the mono face only when their provenance matters.

## Logo

`web/public/logo.svg` is the shared, transparent 1:1 brand mark and favicon.
It depicts one cobalt building block with a single raised stud, using solid
geometric faces and no visible text. Its 64 × 64 viewBox keeps the silhouette
legible at favicon sizes without relying on lettering or thin strokes.

Both sidebar headers display the mark at 32 × 32 px alongside the semibold
product name CTX. Collapsed rails show only the mark at 28 × 28 px. They use
the same `/logo.svg?v=block` URL as the favicon; the
version query refreshes previously cached wordmarks when this design ships.

The SVG has its own light and dark palettes for standalone use. The page sets
`color-scheme` with the active theme so the sidebar image's
`prefers-color-scheme` follows manual theme changes, including the existing
`D` shortcut. The favicon follows the browser's color scheme. Neither surface
requires extra theme state or animation.

## Layout and components

The responsive application shell uses a compact document list, a writing canvas, and an optional AI panel. The public square is a readable list of articles and the public reader limits prose to a 72ch measure. Shared controls are shadcn Base UI components; surfaces use an 8px radius and either a quiet border or a defined small shadow, never both as decoration. Motion stays within 150–200ms and communicates save, publish, or task state only.
