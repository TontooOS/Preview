# Preview

TontooOS document viewer basis: a 960x640 window without a system
decoration bar (traffic lights sit directly on the window), with a top
row (`TrafficLights` plus file name, `Edit`-`Done` / `Save` text actions
and a Finder-style `Toolbar` with open, zoom in, zoom out, share and
annotate icons), an empty state with a centered open button, a text
viewer with line numbers on the left, and a Markdown mode with a
rendered preview plus a raw edit mode.
PDFs open read-only in a page viewer (see [Pdf.md](Pdf.md)).
Images open read-only as pictures (see [Image.md](Image.md)).
Audio files open read-only in a compact player (see [Audio.md](Audio.md)).
Video files open read-only on a player page with a picture
(see [Video.md](Video.md)).
Word documents open read-only as formatted text
(see [Docx.md](Docx.md)).
Presentations open read-only as a slide viewer
(see [Pptx.md](Pptx.md)).
Spreadsheets open read-only as a sheet grid
(see [Xlsx.md](Xlsx.md)).
Saving is manual only. Unsupported files show a hint page instead of
binary garbage.

## Layout

From top to bottom the window contains:

1. Top row: traffic lights directly on the window plus title plus subtitle on the left, `Edit`-`Done` / `Save` plus a `Toolbar` on the right (no decoration bar, no separator)
2. Content stack: empty, text (edit/preview), pdf, image, audio, video, docx, pptx, xlsx, or unsupported page
3. Status line: line count and save state

## Header Toolbar

Finder-style `Toolbar` (`TontooUI`) on the far right of the top row:

| Icon | Behavior |
|---|---|
| `doc.badge.arrow.up.fill` | Opens the native file dialog |
| `plus.magnifyingglass` | Zooms in (PDF font scale, image pixbuf scale) |
| `minus.magnifyingglass` | Zooms out (PDF font scale, image pixbuf scale) |
| `square.and.arrow.up.fill` | No action yet (inert, insensitive) |
| `square.and.pencil` | No action yet (inert, insensitive) |

Rules:

- The toolbar is always visible; the open icon replaces the old `Open`
  text button.
- Zoom icons are sensitive only for PDF and image pages.
- Share and annotate stay insensitive until wired.

```rust
let mut app = App::with_delegate(title, 960, 640, PreviewDelegate { initial });
app.no_window_bar(); // traffic lights are drawn directly on the window
app.auto_color_scheme(); // live Dark/Light follow
app.run();
```

## File Handling

`src/model.rs` classifies by extension and loads text safely:

| Kind | Extensions | Behavior |
|---|---|---|
| `Markdown` | `md`, `markdown` | Rendered preview plus raw edit mode |
| `Text` | `txt`, `json`, `py`, `rs`, `toml`, `log`, ... | Plain preview plus edit mode |
| `Pdf` | `pdf` | Read-only page viewer (see [Pdf.md](Pdf.md)) |
| `Image` | `png`, `jpg`, `gif`, `bmp`, `webp`, `tiff`, `svg`, `ico`, `avif`, ... | Read-only picture viewer (see [Image.md](Image.md)) |
| `Audio` | `mp3`, `wav`, `flac`, `ogg`, `oga`, `opus`, `m4a`, `aac`, `wma`, `aiff`, `aif` | Read-only player (see [Audio.md](Audio.md)) |
| `Video` | `mp4`, `m4v`, `mkv`, `webm`, `mov`, `avi`, `ogv`, `flv`, `wmv`, `mpg`, `mpeg`, `3gp` | Read-only player with picture (see [Video.md](Video.md)) |
| `Document` | `docx`, `odt`, `rtf`, `doc` | Read-only formatted text (see [Docx.md](Docx.md)) |
| `Presentation` | `pptx`, `odp`, `ppt` | Read-only slide viewer (see [Pptx.md](Pptx.md)) |
| `Spreadsheet` | `xlsx`, `xls`, `csv`, `tsv`, `ods` | Read-only sheet grid (see [Xlsx.md](Xlsx.md)) |
| `Unsupported` | `zip`, ... | Hint page, no binary load |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn load_text(path: &Path) -> Result<String, String>;
pub fn save_text(path: &Path, content: &str) -> Result<(), String>;
impl PdfDoc {
  pub fn open(path: &Path) -> Result<Self, String>;
  pub fn page_count(&self) -> usize;
  pub fn page_text(&self, index: usize) -> Result<String, String>;
}
pub fn clamp_page(index: usize, total: usize) -> usize;
```

Rules:

- Files over 8 MiB are rejected with a reason.
- Content with NUL bytes is rejected as binary.
- Non UTF-8 files are rejected with a reason.
- Unknown extensions show the unsupported page with the reason plus a
  hint to try a text file.

## Opening Files

Files open two ways, per the CLI plus dialog decision:

| Way | Source |
|---|---|
| CLI argument | `preview /path/to/file.txt` (first existing arg) |
| Native dialog | `Open` button, empty state button, or unsupported page button |

Opening another file with unsaved changes asks first (Cancel / Save /
Don't Save). The dialog is the native GTK file chooser.

## Markdown Preview

`src/markdown.rs` renders into a read-only `GtkTextView` with text tags
(SF Pro Display, larger bold headings, monospace code):

| Syntax | Rendering |
|---|---|
| `#`, `##`, `###` | Scaled bold headings |
| `-`, `*`, `1.` | Bullet or numbered list rows |
| `> ` | Indented italic quote |
| `**bold**` | Bold tag |
| `*italic*` | Italic tag |
| `` `code` `` | Monospace tag with background |
| ` ``` ` | Fenced block in monospace |
| `[text](url)` | Bold label plus dim URL |
| `---` | Rule line |

Edit mode always shows the raw Markdown source.

## Edit Mode and Manual Save

Edit mode shows the raw text in a monospace editor with a line-number
gutter on the left (gutter shares the editor vertical adjustment).
Preview mode is read-only.

| Action | Behavior |
|---|---|
| `Edit` / `Done` | Toggles raw edit and read-only preview |
| `Save` button | Writes the buffer back, clears dirty |
| `Ctrl+S` | Same as `Save` (global shortcut) |
| Window close | Dirty files get Cancel / Save / Don't Save |

There is no autosave: the status line shows `%lines% lines` when saved
and `%lines% lines, unsaved changes` when dirty. Leaving edit mode with
unsaved changes keeps the buffer; Markdown preview renders the current
buffer so the change is visible immediately.

## Localization

`src/lang.rs` loads a flat `HashMap<String, String>` from `lang/` based
on `LANGUAGE` / `LC_ALL` / `LANG` / `/etc/locale.conf`. Only `en_us` and
`de_de` exist. Missing keys return the key itself.

| Key | `en_us` | `de_de` |
|---|---|---|
| `app.title` | `Preview` | `Vorschau` |
| `action.open_file` | `Open File` | `Datei öffnen` |
| `action.edit` | `Edit` | `Bearbeiten` |
| `action.save` | `Save` | `Speichern` |
| `close.dont_save` | `Don't Save` | `Nicht speichern` |

## Styling

All text uses SF Pro Display (`SF_PRO`). The window follows the live
system color scheme (Dark `#1d1d1d`, Light `#ececec`) via
`app.auto_color_scheme()`.

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Image.md](Image.md) – read-only picture viewer
- [Audio.md](Audio.md) – read-only audio player
- [Video.md](Video.md) – read-only video player
- [Docx.md](Docx.md) – read-only word document viewer
- [Pptx.md](Pptx.md) – read-only presentation slide viewer
- [Xlsx.md](Xlsx.md) – read-only spreadsheet sheet viewer
