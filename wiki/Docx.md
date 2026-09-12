# Docx

Word documents open read-only on a dedicated document page: a title plus a
file info line plus a formatted read-only view (headings, bold/italic,
bulleted lists, tables as a plain grid). Parsing uses only crates already
in the `Cargo.lock` closure (`flate2` for deflated ZIP entries, `quick-xml`
for the document XML), so no new crate family is introduced. Corrupt files,
password-protected packages and legacy `.doc` files stay on the document
page and show an honest hint with the reason; they never fall through to
the unsupported page and never crash. Documents are never editable and
never saved.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed files still land on the document page. Document
sniffing only matches Word and ODT packages (never other ZIP files such as
spreadsheets or presentations), RTF markup and OLE bytes.

| Kind | Extensions | Behavior |
|---|---|---|
| `Document` | `docx`, `odt`, `rtf`, `doc` | Read-only formatted text, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_document(bytes: &[u8]) -> bool;
pub fn is_rtf_markup(bytes: &[u8]) -> bool;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Document` when the file header carries known document magic: a
ZIP package (`PK\x03\x04`) holding `word/` part names (`.docx`), a ZIP
package holding the OpenDocument text MIME type (`.odt`), RTF markup
(`{\rtf`) or a legacy OLE compound file (`.doc`, also used by
password-protected OOXML packages). Other ZIP files (spreadsheets,
presentations, plain archives) never sniff as documents.

Missing files are reported when the document is loaded, not during
classification, so the page can show the reason hint.

## Supported Formats

Exact coverage per extension (best-effort means graceful degradation to
plain text, never binary garbage and never a crash):

| Extension | Level | Notes |
|---|---|---|
| `docx` | Full | Paragraphs, headings, bold/italic, bulleted and numbered lists, tables as a plain grid |
| `odt` | Best-effort | Paragraphs, headings, bold/italic via automatic styles, lists as bullets, tables as a plain grid |
| `rtf` | Best-effort | Paragraphs, bold/italic, tables as `a \| b` rows; metadata destinations skipped |
| `doc` | Hint only | Legacy OLE has no decoder; shows the legacy hint (includes password-protected files) |

```rust
pub const DOCUMENT_EXTENSIONS: &[&str] = &["docx", "odt", "rtf", "doc"];
```

> **Note:** Numbered lists render as bullets in every format (numbering is
> not resolved). Deleted tracked text (`w:del`) and field codes
> (`w:instrText`) are skipped. Files over 8 MiB are rejected with a reason,
> matching the text budget (`MAX_TEXT_BYTES`); each decompressed XML part
> is capped the same way and rendering stops after 20,000 blocks with a
> `docx.truncated` hint line.

## Document Loading

`src/docx.rs` loads the whole file (capped at 8 MiB) and routes by content
sniffing, so a renamed file still parses when its bytes allow it:

### load_document

```rust
pub fn load_document(path: &Path) -> Result<Document, String>;
```

Loads a word document for read-only preview and returns its blocks.

- Returns `Ok` with the format label, the blocks in document order, the
  file size and the truncation flag.
- Returns `Err` with a human-readable reason when the path is a directory,
  the file is missing, empty or too large, the package is
  password-protected, the format is legacy `.doc`, the archive holds no
  word document (spreadsheets and presentations name their kind), or the
  content is corrupt.

```rust
let doc = docx::load_document(path)?;
println!("{} {} {}", doc.format, doc.paragraph_count(), doc.word_count());
```

### Document blocks

| Type | Shape | Description |
|---|---|---|
| `Heading` | `level: u8`, `spans` | Heading level 1..=3 plus inline runs |
| `Paragraph` | `spans` | Normal paragraph plus inline runs |
| `Bullet` | `spans` | List item (bulleted and numbered alike) |
| `Table` | `Vec<Vec<String>>` | Table as a plain grid of plain-text cells |

Each `Span` holds `text` plus `bold` and `italic` flags.

```rust
pub fn paragraph_count(&self) -> usize;
pub fn word_count(&self) -> usize;
```

`paragraph_count` counts headings, paragraphs and list items (tables
excluded). `word_count` counts whitespace-separated words over runs and
table cells.

## Parsers

Pure parsers over bytes, unit tested with synthetic in-test samples
(minimal ZIP archives are built by a test-only stored/deflated writer):

```rust
pub fn document_format_label(ext: &str) -> &'static str;
```

- `document_format_label` maps `docx`/`odt`/`rtf`/`doc` to
  `DOCX`/`ODT`/`RTF`/`DOC` (`Document` when unknown); it lives in
  `src/model.rs` next to the audio and video labels.
- The `.docx` parser reads `word/document.xml`: `Title`/`Heading1` maps to
  level 1, `Heading2` to level 2, deeper styles to level 3; `w:b`/`w:i`
  set the run flags (`w:val="false"` respected); `w:numPr` marks bullets;
  `w:tbl` builds grid rows.
- The `.odt` parser reads `content.xml`: `text:h` levels come from
  `text:outline-level`; bold/italic come from automatic
  `style:text-properties`; `text:list` items become bullets;
  `table:table` builds grid rows.
- The `.rtf` parser decodes Windows-1252 bytes, ends paragraphs on `\par`,
  toggles flags on `\b`/`\i`, decodes `\'hh` and `\uN` escapes and turns
  `\cell` into ` | ` separators.
- The ZIP reader parses the end-of-central-directory record plus the
  central directory and extracts only the needed entries (stored or
  deflated via `flate2` with a cap); other methods and truncated archives
  report honest errors.

## Document UI

`src/views/preview.rs` adds a `docx` page to the content stack with a
title, an info line and the formatted view (SF Pro Display). The page is
only visible for documents; `Edit` and `Save` are hidden and `Ctrl+S` is a
no-op for documents.

| Control | Key | Behavior |
|---|---|---|
| Title | — | File name in `.docx-title` (SF Pro Display, bold) |
| Info line | `status.docx` | `%format%, %paragraphs% paragraphs, %words% words, %size%, read-only` |
| Body | — | Read-only `GtkTextView` with `h1`/`h2`/`h3`, `bold`, `italic` and `dim` tags |
| Tables | — | Plain `a \| b` grid rows with a dim separator |
| Truncated | `docx.truncated` | Dim `Large document: showing the first %count% blocks` line at the end |

The status line shows the same text as the info line. The window follows
the live system color scheme (Dark `#1d1d1d`, Light `#ececec`) via
`app.auto_color_scheme()`; no background is hardcoded.

## Error Cases

Broken documents stay on the `docx` page and show `docx.load_failed` with
the reason; they never fall through to the unsupported page and never
crash.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Empty file | `file is empty` |
| Oversized file | `file is too large for the document viewer (over 8 MiB)` |
| Password-protected package | `file is encrypted (password-protected); ...` |
| Legacy `.doc` | `legacy .doc format is not parsed ...; convert it to .docx ...` |
| Spreadsheet/presentation archive | `this is a spreadsheet/presentation, only word documents (.docx) ...` |
| Corrupt content | Parse or decompression reason, never a crash |

> **Note:** A `.docx`/`.odt` file whose bytes are OLE is reported as
> password-protected, since Office encryption produces OLE packages. Full
> files are only read up to the 8 MiB budget; header probing never applies
> here because formatted text needs the whole part.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%reason%`, `%count%`, `%format%`,
`%paragraphs%`, `%words%`, `%size%`). Only `en_us` and `de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `docx.load_failed` | `Cannot open this document: %reason%` | `Dieses Dokument kann nicht geöffnet werden: %reason%` |
| `docx.truncated` | `Large document: showing the first %count% blocks` | `Großes Dokument: die ersten %count% Blöcke werden gezeigt` |
| `status.docx` | `%format%, %paragraphs% paragraphs, %words% words, %size%, read-only` | `%format%, %paragraphs% Absätze, %words% Wörter, %size%, schreibgeschützt` |

```json
{
  "status.docx": "%format%, %paragraphs% paragraphs, %words% words, %size%, read-only"
}
```

## Dependencies

Parsing uses `flate2` (raw deflate for method-8 ZIP entries) and
`quick-xml` (document XML), both already in the `Cargo.lock` closure via
other crates; they are now direct dependencies. Declared in `Cargo.toml`:

```toml
flate2 = "1.1"
quick-xml = "0.37"
```

No existing dependency version was changed. No new system package is
needed. The versions stay user-managed: bump them only on request.

## Usage / Example

Open a word document from the command line or the native file dialog:

```bash
cargo run -- /path/to/report.docx
```

Headings render large and bold, list items show bullets, tables show as
`A1 | B1` grid rows. There is no edit mode and no save path for
documents.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Document);
let doc = docx::load_document(path)?;
docx::render_into(&buffer, &doc);
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Image.md](Image.md) – read-only picture viewer
- [Audio.md](Audio.md) – read-only audio player
- [Video.md](Video.md) – read-only video player
