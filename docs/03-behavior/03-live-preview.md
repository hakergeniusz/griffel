# 03/03 — Live preview

## 1. Overview

The preview renders the document as HTML in an embedded web view. Rendering
is delegated to an external document converter (see `00-overview.md`); the
app passes the document text, the chosen input flavor, and presentation
options to it, and displays the resulting HTML. The preview is live: it
re-renders as the document changes.

## 2. Activation and layouts

- Preview is toggled per window (Ctrl+P, headerbar/menu items per
  `02-ui-layout.md`). The on/off state at window creation is a remembered
  preference.
- Layout modes: half-width (default), full-width, half-height, windowed —
  see `02-ui-layout.md` §4. Mode is a remembered preference; changes animate.
- In windowed mode the preview lives in its own window; closing that window
  switches the main window's preview off.

## 3. Update pipeline

1. When the preview becomes visible (or the flavor/security/appearance
   settings change), the current text is converted to HTML and loaded into
   the web view.
2. While the user types, updates are debounced (a burst of keystrokes
   produces one re-render shortly after it settles).
3. While a re-render is loading, the previous render stays visible (shown as
   a frozen image, crossfaded into the new render on completion) so the pane
   never flashes blank.
4. A spinner page is shown instead of the web view whenever the preview is
   waiting for its first render or the file's security level is not yet
   decided (§5).

The conversion happens on a background thread; at most one queued conversion
runs at a time and only the latest text is rendered (stale results are
dropped).

## 4. Conversion details (behavioral)

- The input flavor is the input format preference (`10-preferences.md`).
- For a **restricted** render (§5) raw HTML is disabled in the input flavor
  (the converter's raw-HTML extension is turned off), so HTML in the source
  renders as plain text.
- The conversion embeds the app's stylesheet matching the current color
  scheme (`09-appearance.md`), enables MathJax-style math rendering, and
  rewrites relative image/link paths to be relative to the document's folder
  so relative assets resolve.
- The rendered page reports when it has finished rendering; scroll
  synchronization starts only after that point.

## 5. Security model

When a file is opened, the app decides whether the preview may run raw HTML
and scripts:

1. If the policy preference is "always restricted" or "always unrestricted",
   that level is applied to every file without asking.
2. If the policy is "always ask" (default): the opened text is scanned for
   raw HTML tags (any HTML element markup, case-insensitive). Files with
   none are previewed restricted automatically (no dialog). Files with HTML
   are marked "ask".
3. When preview is activated for an "ask" file, the preview pane shows the
   security warning page (`02-ui-layout.md` §4) offering two choices:
   load unrestricted, or load restricted (emphasized). The chosen level
   applies to the current file until another file is loaded; the global
   policy is not changed by this choice.
4. New (never-saved) documents are unrestricted.

The embedded web view additionally: disables developer tools in release
builds, opens clicked web links in the external browser (never navigating
itself), and removes navigation entries (reload/back/forward/stop) from its
context menu.

## 6. Scroll synchronization

- When enabled (preference, default on), the editor's scroll fraction (0–1)
  and the preview's scroll fraction are kept in sync **bidirectionally**:
  scrolling either pane scrolls the other proportionally.
- Sync is re-established after each re-render completes, starting from the
  editor's current fraction.
- Sync is disabled when the setting is off; toggling it reloads the preview.

## 7. Responsiveness

- Preview conversion and rendering must never block typing.
- The preview pane must appear within the layout transition duration plus
  one conversion round-trip for typical documents.
