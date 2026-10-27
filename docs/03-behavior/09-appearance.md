# 03/09 — Appearance: color schemes

## 1. Schemes

Four color schemes, selectable in the main menu (`02-ui-layout.md` §9):
**System** (follow the desktop preference; default), **Light**, **Sepia**,
**Dark**.

- The choice is a remembered preference and applies immediately to all open
  windows.
- System support is detected; on platforms without a system color-scheme
  preference the "system" option is hidden.
- Sepia is a warm, paper-toned variant of the light theme applied to both
  the app chrome and the editor/preview.

## 2. What restyles

- **App chrome** follows the platform style (light/dark per scheme; sepia
  adds a stylesheet override on top of the light style).
- **Editor**: the text highlight palette switches with the scheme (each
  scheme defines editor colors: background, foreground, code background
  tint, accent/link color, dimmed-text color). In-place Markdown
  highlighting colors update live when the scheme changes, without reload.
- **Preview and HTML export**: the preview page uses the stylesheet
  matching the current scheme; a dark scheme also switches the converter's
  syntax-highlighting style to a dark variant. Changing the scheme reloads
  the preview.

## 3. High contrast

The platform's high-contrast preference is honored for the app chrome
(standard platform behavior); the editor palette follows the active scheme
regardless.
