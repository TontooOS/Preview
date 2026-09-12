# Preview

TontooOS document viewer basis: a 960x640 window with a header (`Preview`
title plus file name, `Open` / `Edit`-`Done` / `Save` actions), an empty
state with a centered open button, a text viewer with line numbers on the
left, and a Markdown mode with a rendered preview plus a raw edit mode.
PDFs open read-only in a page viewer (see [Pdf.md](Pdf.md)).
Saving is manual only. Unsupported files show a hint page instead of
binary garbage.

## Layout

From top to bottom the window contains:

1. Header row: title plus subtitle on the left, actions on the right
2. Content stack: empty, text (edit/preview), pdf, or unsupported page
3. Status line: line count and save state

```rust
let mut app = App::with_delegate(title, 960, 640, PreviewDelegate { initial });
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
| `Unsupported` | `png`, `mp3`, `xlsx`, ... | Hint page, no binary load |

```rust
pub fn classify(path: &Path) -> FileKind;
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
