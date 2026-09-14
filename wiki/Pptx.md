# Pptx

Presentations open read-only on a dedicated slide viewer page: a toolbar
with previous/next navigation plus a slide indicator, a title plus a file
info line plus a formatted read-only view (large bold title, body text,
bold/italic, bulleted lists with outline levels, tables as a plain grid,
dim speaker notes). Parsing uses only crates already in the `Cargo.lock`
closure (`flate2` for deflated ZIP entries, `quick-xml` for the slide
XML), so no new crate family is introduced. Corrupt files,
password-protected packages and legacy `.ppt` files stay on the
presentation page and show an honest hint with the reason; they never fall
through to the unsupported page, never show binary garbage and never
crash. Presentations are never editable and never saved.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed files still land on the slide viewer. Presentation
sniffing only matches PowerPoint and ODP packages (never other ZIP files
such as spreadsheets or word documents).

| Kind | Extensions | Behavior |
|---|---|---|
| `Presentation` | `pptx`, `odp`, `ppt` | Read-only slide viewer, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_presentation(bytes: &[u8]) -> bool;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Presentation` when the file header carries known presentation
magic: a ZIP package (`PK\x03\x04`) holding `ppt/` part names (`.pptx`)
or a ZIP package holding the OpenDocument presentation MIME type
(`.odp`). Other ZIP files (spreadsheets, word documents, plain archives)
never sniff as presentations. Legacy `.ppt` (OLE) files have no cheap
distinguishing magic, so they are only classified by extension.

Missing files are reported when the deck is loaded, not during
classification, so the page can show the reason hint.

## Supported Formats

Exact coverage per extension (best-effort means graceful degradation to
plain text, never binary garbage and never a crash):

| Extension | Level | Notes |
|---|---|---|
| `pptx` | Full | Slide order via `presentation.xml` plus relationships, title plus body text, bold/italic, bullets with outline levels, tables as a plain grid, speaker notes |
| `odp` | Best-effort | Slides in `draw:page` order, first heading as title, bold/italic via automatic styles, nested lists as leveled bullets, tables as a plain grid, `presentation:notes` as speaker notes |
| `ppt` | Hint only | Legacy OLE has no decoder; shows the legacy hint (includes password-protected files) |

```rust
pub const PRESENTATION_EXTENSIONS: &[&str] = &["pptx", "odp", "ppt"];
```

> **Note:** Numbered lists render as bullets in every format (numbering is
> not resolved). Slide field codes (`a:fld`, e.g. automatic slide numbers)
> are skipped. Decks over 8 MiB are rejected with a reason, matching the
> text budget (`MAX_TEXT_BYTES`); each decompressed XML part is capped the
> same way, parsing stops after 500 slides and 2,000 blocks per slide with
> a `pptx.truncated` hint line on the last slide.

## Deck Loading

`src/pptx.rs` loads the whole file (capped at 8 MiB) and routes by
content sniffing, so a renamed file still parses when its bytes allow it:

### load_presentation

```rust
pub fn load_presentation(path: &Path) -> Result<Deck, String>;
```

Loads a presentation for read-only preview and returns its slides in
presentation order.

- Returns `Ok` with the format label, the slides in presentation order,
  the file size and the truncation flag.
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing, empty or too large, the package is
  password-protected, the format is legacy `.ppt`, the archive holds no
  presentation (spreadsheets and word documents name their kind), or the
  content is corrupt.

```rust
let deck = pptx::load_presentation(path)?;
println!("{} {} {}", deck.format, deck.slide_count(), deck.word_count());
```

### Deck shapes

| Type | Shape | Description |
|---|---|---|
| `Slide` | `title`, `blocks`, `notes` | Title runs, body blocks, plain-text speaker notes |
| `Paragraph` | `spans` | Normal body paragraph plus inline runs |
| `Bullet` | `level: u8`, `spans` | List item with its outline level (0 is top level) |
| `Table` | `Vec<Vec<String>>` | Table as a plain grid of plain-text cells |

Each `Span` holds `text` plus `bold` and `italic` flags.

```rust
pub fn slide_count(&self) -> usize;
pub fn word_count(&self) -> usize;
```

`slide_count` counts the slides. `word_count` counts
whitespace-separated words over titles, blocks and notes.

## Parsers

Pure parsers over bytes, unit tested with synthetic in-test samples
(minimal ZIP archives are built by a test-only stored writer):

```rust
pub fn presentation_format_label(ext: &str) -> &'static str;
```

- `presentation_format_label` maps `pptx`/`odp`/`ppt` to
  `PPTX`/`ODP`/`PPT` (`Presentation` when unknown); it lives in
  `src/model.rs` next to the audio, video and document labels.
- The `.pptx` loader reads slide order from `ppt/presentation.xml`
  (`sldId` relationship ids in order) through
  `ppt/_rels/presentation.xml.rels` targets; when the relationships part
  is missing it falls back to sorted `ppt/slides/slide*.xml` entry names.
  Only `title`/`ctrTitle` placeholders become the slide title (the first
  non-empty one wins); `a:rPr b`/`i` attributes set the run flags;
  `a:buChar`/`a:buAutoNum`/`a:buBlip` mark bullets with the `a:pPr lvl`
  outline level; `a:tbl` builds grid rows. Speaker notes come from the
  matching `ppt/notesSlides/notesSlideN.xml` part (best-effort: missing
  notes stay empty).
- The `.odp` parser reads `content.xml`: each `draw:page` is one slide,
  the first `text:h` becomes the title, bold/italic come from automatic
  `style:text-properties`, nested `text:list` items become bullets with
  their nesting level, `table:table` builds grid rows and
  `presentation:notes` paragraphs become speaker notes (lists inside
  notes stay notes, never bullets).
- The ZIP reader parses the end-of-central-directory record plus the
  central directory and extracts only the needed entries (stored or
  deflated via `flate2` with a cap); other methods and truncated archives
  report honest errors. It mirrors the `src/docx.rs` reader.

## Slide UI

`src/views/preview.rs` adds a `pptx` page to the content stack with a
navigation toolbar, a title, an info line and the formatted slide view
(SF Pro Display). The page is only visible for presentations; `Edit` and
`Save` are hidden and `Ctrl+S` is a no-op for presentations. Navigation
mirrors the PDF page toolbar pattern (previous/next plus indicator).

| Control | Key | Behavior |
|---|---|---|
| Previous | `pptx.prev` | Goes to the previous slide |
| Next | `pptx.next` | Goes to the next slide |
| Indicator | `pptx.slide` | `Slide %page% of %total%` |
| Title | — | File name in `.pptx-title` (SF Pro Display, bold) |
| Info line | `status.pptx` | `%format%, %slides% slides, %words% words, %size%, read-only` |
| Body | — | Read-only `GtkTextView` with `h1`, `bold`, `italic` and `dim` tags |
| Bullets | — | Outline indent plus a dim `•` marker |
| Tables | — | Plain `a \| b` grid rows with a dim separator |
| Notes | `pptx.notes` | Dim `Notes` section below the slide body |
| Truncated | `pptx.truncated` | Dim `Large deck: showing the first %count% slides` line on the last slide |

There is no bottom status line. The window follows
the live system color scheme (Dark `#1d1d1d`, Light `#ececec`) via
`app.auto_color_scheme()`; no background is hardcoded.

## Error Cases

Broken presentations stay on the `pptx` page and show
`pptx.load_failed` with the reason; they never fall through to the
unsupported page and never crash.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Empty file | `file is empty` |
| Oversized file | `file is too large for the slide viewer (over 8 MiB)` |
| Password-protected package | `file is encrypted (password-protected); ...` |
| Legacy `.ppt` | `legacy .ppt format is not parsed ...; convert it to .pptx ...` |
| Spreadsheet/word archive | `this is a spreadsheet/word document, only presentations (.pptx) ...` |
| Corrupt content | Parse or decompression reason, never a crash |

> **Note:** A `.pptx`/`.odp` file whose bytes are OLE is reported as
> password-protected, since Office encryption produces OLE packages. Full
> files are only read up to the 8 MiB budget; header probing never applies
> here because formatted slides need the whole part.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%page%`, `%total%`, `%reason%`, `%count%`,
`%format%`, `%slides%`, `%words%`, `%size%`). Only `en_us` and `de_de`
exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `pptx.prev` | `Previous` | `Zurück` |
| `pptx.next` | `Next` | `Weiter` |
| `pptx.slide` | `Slide %page% of %total%` | `Folie %page% von %total%` |
| `pptx.load_failed` | `Cannot open this presentation: %reason%` | `Diese Präsentation kann nicht geöffnet werden: %reason%` |
| `pptx.notes` | `Notes` | `Notizen` |
| `pptx.truncated` | `Large deck: showing the first %count% slides` | `Große Präsentation: die ersten %count% Folien werden gezeigt` |
| `status.pptx` | `%format%, %slides% slides, %words% words, %size%, read-only` | `%format%, %slides% Folien, %words% Wörter, %size%, schreibgeschützt` |

```json
{
  "pptx.slide": "Slide %page% of %total%"
}
```

## Dependencies

Parsing uses `flate2` (raw deflate for method-8 ZIP entries) and
`quick-xml` (slide XML), both already direct dependencies from the
document work (same `Cargo.lock` closure). Declared in `Cargo.toml`:

```toml
flate2 = "1.1"
quick-xml = "0.37"
```

No new dependency was added and no existing dependency version was
changed. No new system package is needed. The versions stay
user-managed: bump them only on request.

## Usage / Example

Open a presentation from the command line or the native file dialog:

```bash
cargo run -- /path/to/deck.pptx
```

The title renders large and bold, list items show indented bullets,
tables show as `A1 | B1` grid rows and speaker notes show dim below the
body. There is no edit mode and no save path for presentations.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Presentation);
let deck = pptx::load_presentation(path)?;
pptx::render_slide_into(&buffer, &deck, 0);
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Image.md](Image.md) – read-only picture viewer
- [Audio.md](Audio.md) – read-only audio player
- [Video.md](Video.md) – read-only video player
- [Docx.md](Docx.md) – read-only word document viewer
