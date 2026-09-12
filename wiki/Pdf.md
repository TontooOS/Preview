# Pdf

PDF files open read-only in a dedicated page viewer: the toolbar offers
previous/next navigation with a page indicator (`Page 2 of 10`), zoom
in/out steps and a fit-width toggle. Page content is extracted lazily
(current page only) as a text layer, so documents with huge page counts
stay cheap. PDFs are never editable and never show the unsupported page;
broken files show an error hint with the reason instead.

## File Detection

`src/model.rs` classifies by extension; `pdf` (any case) maps to the new
`FileKind::Pdf` variant. PDF bytes are never loaded as text.

| Kind | Extensions | Behavior |
|---|---|---|
| `Pdf` | `pdf` | Read-only page viewer, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
```

Missing files are reported when the document is opened, not during
classification, so the viewer can show the reason hint.

## PdfDoc

`src/model.rs` wraps `lopdf::Document` with a sorted page table. Only
metadata is kept in memory; page text is extracted on demand.

### PdfDoc::open

```rust
impl PdfDoc {
  pub fn open(path: &Path) -> Result<Self, String>;
}
```

Opens a PDF file and reads its page table.

- Returns `Ok` with the document and its page count.
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing, the file is corrupt, the file has no
  readable pages, or the file is encrypted (password-protected files
  cannot be opened without a password).

```rust
let doc = model::PdfDoc::open(path)?;
println!("pages: {}", doc.page_count());
```

### PdfDoc::page_count

```rust
pub fn page_count(&self) -> usize;
```

Returns the number of pages in the document.

### PdfDoc::page_text

```rust
pub fn page_text(&self, index: usize) -> Result<String, String>;
```

Extracts the text layer of one page (0-based index).

- Returns `Ok` with the page text; empty when the page holds no
  extractable text (e.g. scanned images), in which case the viewer shows
  the `pdf.empty_page` hint.
- Returns `Err` when the index is out of range or extraction fails.

```rust
let text = doc.page_text(0)?;
```

### clamp_page

```rust
pub fn clamp_page(index: usize, total: usize) -> usize;
```

Clamps a 0-based page index into `0..total`. Returns `0` when the
document is empty. Used by the previous/next buttons so navigation can
never leave the valid range.

## Viewer UI

`src/views/preview.rs` adds a `pdf` page to the content stack with a
toolbar and a read-only `GtkTextView` (SF Pro Display). The toolbar is
only visible for PDFs; `Edit` and `Save` are hidden and `Ctrl+S` is a
no-op for PDFs.

| Control | Key | Behavior |
|---|---|---|
| `Previous` | `pdf.prev` | Go to the previous page, disabled on page 1 |
| `Page %page% of %total%` | `pdf.page` | Current page indicator |
| `Next` | `pdf.next` | Go to the next page, disabled on the last page |
| `Zoom Out` / `Zoom In` | `pdf.zoom_out` / `pdf.zoom_in` | Font scale in 0.25 steps, clamped to 0.5–3.0 |
| `Fit Width` | `pdf.fit` | Toggle between wrapped (fit) and unwrapped text |

The status line shows `%count% pages, read-only` (`status.pdf`). The
window follows the live system color scheme (Dark `#1d1d1d`, Light
`#ececec`) via `app.auto_color_scheme()`; no background is hardcoded.

## Error Cases

Broken PDFs stay on the `pdf` page and show `pdf.load_failed` with the
reason; they never fall through to the unsupported page.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Corrupt file | `cannot parse PDF: ...` |
| Encrypted file | Password hint, never crashes, no password prompt |
| Empty page | `pdf.empty_page` hint inside the page view |

> **Note:** Text extraction is a text layer only; scanned pages without
> embedded text show the empty-page hint instead of raster images.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%page%`, `%total%`, `%count%`, `%reason%`).
Only `en_us` and `de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `pdf.prev` | `Previous` | `Zurück` |
| `pdf.next` | `Next` | `Weiter` |
| `pdf.page` | `Page %page% of %total%` | `Seite %page% von %total%` |
| `pdf.zoom_in` | `Zoom In` | `Vergrößern` |
| `pdf.zoom_out` | `Zoom Out` | `Verkleinern` |
| `pdf.fit` | `Fit Width` | `Breite anpassen` |
| `pdf.load_failed` | `Cannot open this PDF: %reason%` | `Dieses PDF kann nicht geöffnet werden: %reason%` |
| `pdf.empty_page` | `This page contains no extractable text.` | `Diese Seite enthält keinen extrahierbaren Text.` |
| `status.pdf` | `%count% pages, read-only` | `%count% Seiten, schreibgeschützt` |

```json
{
  "pdf.page": "Page %page% of %total%"
}
```

## Dependencies

PDF parsing uses the pure-Rust `lopdf` crate (declared in `Cargo.toml`,
resolved through `Cargo.lock`):

```toml
lopdf = "0.45"
```

No new system dependency is required: `lopdf` needs no C library, so
the ISO package list stays unchanged. Page rendering is text-layer
extraction, not rasterization, which keeps the viewer dependency-free.

## Usage / Example

Open a PDF from the command line or the native file dialog:

```bash
cargo run -- /path/to/document.pdf
```

Navigate with `Previous` / `Next`, read the `Page 2 of 10` indicator,
adjust the size with `Zoom In` / `Zoom Out`, and toggle `Fit Width`
for long unwrapped lines. There is no edit mode and no save path for
PDFs.

```rust
let kind = model::classify(path);
assert_eq!(kind, model::FileKind::Pdf);
let doc = model::PdfDoc::open(path)?;
let first = doc.page_text(0)?;
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Image.md](Image.md) – read-only picture viewer
