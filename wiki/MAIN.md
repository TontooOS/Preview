# Preview – Wiki

Preview is the TontooOS document viewer basis: a 939x692 TontooUI window
with an empty state (centered open button), a text viewer with line
numbers and a rendered Markdown preview plus raw edit mode. Saving is
manual only (`Save` button, `Ctrl+S`, close dialog with Cancel / Save /
Don't Save). Files open via CLI (`preview /path/to/file`) or the native
file dialog. It follows the live system color scheme and loads `en_us` /
`de_de` strings from `lang/`.

- Repository: https://github.com/TontooOS/TontooOS
- License: TCL v26.1
- Version: 26.1.0

## Feature Index

| Feature | File | Description |
|---|---|---|
| Main index | [MAIN.md](MAIN.md) | This page |
| Rules | [RULE.md](RULE.md) | Development and usage rules |
| Preview | [Preview.md](Preview.md) | Text viewer, Markdown preview/edit, manual save and localization |
| Pdf | [Pdf.md](Pdf.md) | Read-only PDF page viewer with navigation, zoom and fit width |
| Image | [Image.md](Image.md) | Read-only picture viewer with zoom and fit window |
| Audio | [Audio.md](Audio.md) | Read-only audio player with play/pause, seek, volume and file info |
| Video | [Video.md](Video.md) | Read-only video player with picture, play/pause, seek, volume and file info |
| Docx | [Docx.md](Docx.md) | Read-only word document viewer with formatted text, lists and tables |
| Pptx | [Pptx.md](Pptx.md) | Read-only presentation slide viewer with navigation, formatted text and speaker notes |
| Xlsx | [Xlsx.md](Xlsx.md) | Read-only spreadsheet sheet viewer with tabs, grid, column letters and row numbers |

## Quick Start

Run the Preview window from the repository root:

```bash
cargo run
```

Open a file directly:

```bash
cargo run -- /path/to/file.md
```

The window follows the system theme live (Dark `#1d1d1d`, Light
`#ececec`) and picks German strings when `LANG` starts with `de`.

See [Preview.md](Preview.md) for details.

## Changelog

- 2026-09-14: Empty state cleanup (header title plus subtitle and status
  line stay hidden until a file is open, so the centered title and hint
  show once); open button restyled as a large pill
  (`.preview-open-button`, 220x48, 999px radius, SF Pro 14pt bold).

- 2026-09-13: Pinned default window size via `force_size(939, 692)` so
  the half-monitor cap no longer shrinks the window; fit mode keeps the
  picture inside the viewport without explicit sizing (manual zoom
  scrolls, leaving fit syncs the factor first).

- 2026-09-13: Uniform frame (16px to the window edge on all four
  sides, 8px gaps between header, content and status).
- 2026-09-13: Default window size 939x692; removed the temporary window
  size debug logs.
- 2026-09-13: Removed the in-page image buttons (`Zoom Out`, `Zoom In`,
  `Fit Window`); zoom runs through the header toolbar icons.
- 2026-09-13: Header icon toolbar (Finder-style `Toolbar` with
  `doc.badge.arrow.up.fill`, `plus.magnifyingglass`,
  `minus.magnifyingglass`, `square.and.arrow.up.fill`,
  `square.and.pencil`; open plus PDF/image zoom wired, share and
  annotate inert).
- 2026-09-13: Removed the system decoration bar; traffic lights sit
  directly on the window (`app.no_window_bar()` plus `TrafficLights` in
  the top row, no separator).

- 2026-09-12: Initial Preview basis (empty state, text viewer with line
  numbers, Markdown preview/edit, manual save, CLI plus dialog open,
  `lang/en_us.json` and `lang/de_de.json`).
- 2026-09-12: PDF support (read-only page viewer with previous/next
  navigation, page indicator, zoom in/out plus fit width, lazy per-page
  text extraction, encrypted/corrupt/missing hints, `pdf.*` and
  `status.pdf` keys in `lang/en_us.json` and `lang/de_de.json`).
- 2026-09-12: Image support (read-only picture viewer with `gtk::Picture`,
  zoom in/out plus fit window, raster decoding via the `image` crate with
  magic-byte sniffing, SVG via GTK/librsvg, dimensions plus file size in
  the status line, corrupt/missing hints, `image.*`, `status.image` and
  `status.image_unknown` keys in `lang/en_us.json` and `lang/de_de.json`).
- 2026-09-12: Audio support (read-only player with `gtk::MediaFile`,
  play/pause, seek slider with elapsed/total time, volume slider, format
  plus header duration plus file size in the status line, magic-byte
  sniffing, playback cleanup on file switch and window close,
   corrupt/missing/decoder hints, `audio.*` and `status.audio` keys in
   `lang/en_us.json` and `lang/de_de.json`; needs GStreamer codec plugins
   on the system, see `wiki/Audio.md`).
- 2026-09-12: Video support (read-only player with `gtk::Video` driven by
  `gtk::MediaFile`, play/pause, seek slider with elapsed/total time,
  volume slider, format plus header resolution plus duration plus file
  size in the status line, magic-byte sniffing, playback cleanup on file
  switch and window close, corrupt/missing/decoder/audio-only hints,
  `video.*` and `status.video` keys in `lang/en_us.json` and
  `lang/de_de.json`; needs GStreamer codec plugins on the system, see
  `wiki/Video.md`).
- 2026-09-12: Docx support (read-only word document page with formatted
  text, headings, bold/italic, bulleted lists and tables as a plain grid;
  `docx` fully plus `odt` and `rtf` best-effort plus an honest hint for
  legacy `doc`, magic-byte sniffing, corrupt/password/oversized hints,
   `docx.*` and `status.docx` keys in `lang/en_us.json` and
   `lang/de_de.json`; `flate2` and `quick-xml` are now direct dependencies
   from the existing lock closure, see `wiki/Docx.md`).
- 2026-09-12: Pptx support (read-only presentation slide viewer with
  previous/next navigation, slide indicator, title plus body text,
  bold/italic, leveled bullets, tables as a plain grid and dim speaker
  notes; `pptx` fully plus `odp` best-effort plus an honest hint for
  legacy `ppt`, magic-byte sniffing, corrupt/password/oversized hints,
  `pptx.*` and `status.pptx` keys in `lang/en_us.json` and
  `lang/de_de.json`; no new dependency, see `wiki/Pptx.md`).
- 2026-09-12: Xlsx support (read-only spreadsheet sheet viewer with
  previous/next navigation, sheet indicator, sheet tab switcher, column
  letters plus row numbers, bold header row and right-aligned numbers;
  `xlsx` fully plus `csv` (with semicolon auto-detect), `tsv` and `ods`
  fully plus an honest hint for legacy `xls` (plain `csv`/`tsv` moved
  from the text viewer to the sheet grid), magic-byte sniffing for
  packages, corrupt/password/oversized/binary hints, `xlsx.*` and
  `status.xlsx` keys in `lang/en_us.json` and `lang/de_de.json`; no new
  dependency, see `wiki/Xlsx.md`).
