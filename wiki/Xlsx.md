# Xlsx

Spreadsheets open read-only on a dedicated sheet viewer page: a toolbar
with previous/next navigation plus a sheet indicator, a sheet tab
switcher for multi-sheet workbooks, a title plus a file info line plus a
read-only grid (column letters, row numbers, bold header row,
right-aligned numbers). Parsing uses only crates already in the
`Cargo.lock` closure (`flate2` for deflated ZIP entries, `quick-xml` for
the workbook XML), so no new crate family is introduced. Corrupt files,
password-protected packages and legacy `.xls` files stay on the
spreadsheet page and show an honest hint with the reason; they never
fall through to the unsupported page, never show binary garbage and
never crash. Spreadsheets are never editable and never saved.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed packages still land on the sheet viewer. Plain
`.csv`/`.tsv` files are text and have no magic, so they are only
classified by extension; legacy `.xls` (OLE) shares its magic with
`.doc`, so it is only classified by extension as well.

| Kind | Extensions | Behavior |
|---|---|---|
| `Spreadsheet` | `xlsx`, `xls`, `csv`, `tsv`, `ods` | Read-only sheet grid, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_spreadsheet(bytes: &[u8]) -> bool;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Spreadsheet` when the file header carries known spreadsheet
magic: a ZIP package (`PK\x03\x04`) holding `xl/` part names (`.xlsx`)
or a ZIP package holding the OpenDocument spreadsheet MIME type
(`.ods`). Word documents, presentations and plain archives never sniff
as spreadsheets.

Missing files are reported when the workbook is loaded, not during
classification, so the page can show the reason hint.

> **Note:** Plain `.csv`/`.tsv` files used to open as editable text.
> They now open in the sheet grid instead: a grid shows delimiters,
> quoting and ragged rows honestly, while a text view hides the table
> structure. There is no text fallback for them anymore.

## Supported Formats

Exact coverage per extension (best-effort means graceful degradation to
plain text, never binary garbage and never a crash):

| Extension | Level | Notes |
|---|---|---|
| `xlsx` | Full | Sheet order via `workbook.xml` plus relationships, shared strings, inline strings, booleans as `TRUE`/`FALSE`, numbers and formula results as raw text, grid with column letters plus row numbers |
| `csv` | Full | Pure parser (quoted fields, `""` escapes, embedded line breaks, semicolon auto-detect when no comma exists), single sheet named after the file stem |
| `tsv` | Full | Same parser with tab delimiter, single sheet named after the file stem |
| `ods` | Full | Sheets in `table:table` order, repeated rows and cells expanded, `text:p` text with value attributes as fallback, covered cells as empty cells |
| `xls` | Hint only | Legacy OLE has no decoder; shows the legacy hint (includes password-protected files) |

```rust
pub const SPREADSHEET_EXTENSIONS: &[&str] = &["xlsx", "xls", "csv", "tsv", "ods"];
```

> **Note:** Number formats are not evaluated: serial dates, currencies
> and percentages render as their raw stored text. Cell styling
> (colors, fonts, borders), merged cells, charts, images, pivot tables
> and macros are ignored; merged ranges show only their stored values.
> The first data row renders bold as the header row (cheap headers
> without parsing `xl/styles.xml`). Files over 8 MiB are rejected with
> a reason, matching the text budget (`MAX_TEXT_BYTES`); each
> decompressed XML part is capped the same way, parsing stops after 100
> sheets and every sheet is capped at 1,000 rows by 100 columns with a
> `xlsx.truncated` hint line.

## Workbook Loading

`src/xlsx.rs` loads the whole file (capped at 8 MiB) and routes by
content sniffing, so a renamed file still parses when its bytes allow
it:

### load_workbook

```rust
pub fn load_workbook(path: &Path) -> Result<Workbook, String>;
```

Loads a spreadsheet for read-only preview and returns its sheets in
workbook order.

- Returns `Ok` with the format label, the sheets in workbook order, the
  file size and the truncation flag.
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing, empty or too large, the package is
  password-protected, the format is legacy `.xls`, the archive holds no
  spreadsheet (word documents and presentations name their kind), the
  delimited text is binary or not UTF-8, or the content is corrupt.

```rust
let book = xlsx::load_workbook(path)?;
println!("{} {}", book.format, book.sheet_count());
```

### Workbook shapes

| Type | Shape | Description |
|---|---|---|
| `Sheet` | `name`, `rows` | Display name plus a rectangular grid of plain-text cells |
| `Workbook` | `format`, `sheets`, `file_bytes`, `truncated` | Sheets in workbook order plus file metadata |

```rust
pub fn sheet_count(&self) -> usize;
pub fn sheet_dims(&self, index: usize) -> (usize, usize);
```

`sheet_count` counts the sheets. `sheet_dims` returns the `(rows,
cols)` of one sheet (`(0, 0)` for an out-of-range index or an empty
sheet).

## Parsers

Pure parsers over bytes, unit tested with synthetic in-test samples
(minimal ZIP archives are built by a test-only stored writer):

```rust
pub fn spreadsheet_format_label(ext: &str) -> &'static str;
pub fn parse_delimited(text: &str, delim: char) -> Vec<Vec<String>>;
pub fn parse_cell_ref(cell_ref: &str) -> Option<(usize, usize)>;
pub fn col_letters(col: usize) -> String;
pub fn is_number_text(text: &str) -> bool;
```

- `spreadsheet_format_label` maps `xlsx`/`xls`/`csv`/`tsv`/`ods` to
  `XLSX`/`XLS`/`CSV`/`TSV`/`ODS` (`Spreadsheet` when unknown); it lives
  in `src/model.rs` next to the audio, video, document and presentation
  labels.
- The `.xlsx` loader reads sheet order from `xl/workbook.xml` (sheet
  names plus relationship ids in order) through
  `xl/_rels/workbook.xml.rels` targets; when the relationships part is
  missing it falls back to sorted `xl/worksheets/sheet*.xml` entry
  names (named after their file stem). Shared strings come from
  `xl/sharedStrings.xml`; cells are placed by their `r` reference
  (`B2`, absolute `$B$2` included) with running counters as fallback.
- The `.ods` parser reads `content.xml`: each outermost `table:table`
  is one sheet, `number-rows-repeated` and `number-columns-repeated`
  expand in place, self-closing cells resolve to their value
  attributes, and nested tables are ignored.
- The `.csv`/`.tsv` parser handles quoted fields (delimiter, quotes
  and line breaks inside quotes), `""` escapes and `\r\n`/`\n`/`\r`
  row ends; a trailing newline produces no phantom row. `.csv` files
  use a semicolon when the first line holds no comma (the German Excel
  default), `.tsv` files always use tab.
- The ZIP reader parses the end-of-central-directory record plus the
  central directory and extracts only the needed entries (stored or
  deflated via `flate2` with a cap); other methods and truncated
  archives report honest errors. It mirrors the `src/docx.rs` reader.

## Sheet UI

`src/views/preview.rs` adds an `xlsx` page to the content stack with a
navigation toolbar, sheet tabs, a title, an info line and the
read-only grid (SF Pro Display). The page is only visible for
spreadsheets; `Edit` and `Save` are hidden and `Ctrl+S` is a no-op for
spreadsheets. Navigation mirrors the presentation toolbar pattern
(previous/next plus indicator).

| Control | Key | Behavior |
|---|---|---|
| Previous | `xlsx.prev` | Goes to the previous sheet |
| Next | `xlsx.next` | Goes to the next sheet |
| Indicator | `xlsx.sheet` | `Sheet %page% of %total%: %name%` |
| Tabs | — | One toggle button per sheet, only for multi-sheet workbooks |
| Title | — | File name in `.xlsx-title` (SF Pro Display, bold) |
| Info line | `status.xlsx` | `%format%, %sheets% sheets, %rows%, %size%, read-only` (`%rows%` is `RxC` or `empty`) |
| Grid | — | Corner cell plus column letters (`A`, `B`, ..., `AA`), row numbers, bold header row, right-aligned numbers, cells capped at 30 characters wide |
| Empty | `xlsx.empty_sheet` | Dim `This sheet contains no data.` line for sheets without cells |
| Truncated | `xlsx.truncated` | Dim `Large sheet: showing the first %rows% rows and %cols% columns` line |

The status line shows the same text as the info line. The window follows
the live system color scheme (Dark `#1d1d1d`, Light `#ececec`) via
`app.auto_color_scheme()`; no background is hardcoded.

## Error Cases

Broken spreadsheets stay on the `xlsx` page and show
`xlsx.load_failed` with the reason; they never fall through to the
unsupported page and never crash.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Empty file | `file is empty` |
| Oversized file | `file is too large for the sheet viewer (over 8 MiB)` |
| Password-protected package | `file is encrypted (password-protected); ...` |
| Legacy `.xls` | `legacy .xls format is not parsed ...; convert it to .xlsx ...` |
| Word/presentation archive | `this is a word document/presentation, only spreadsheets (.xlsx) ...` |
| Binary or non UTF-8 delimited text | `file looks like binary data` / `file is not valid UTF-8 text` |
| Corrupt content | Parse or decompression reason, never a crash |

> **Note:** An `.xlsx`/`.ods` file whose bytes are OLE is reported as
> password-protected, since Office encryption produces OLE packages.
> Full files are only read up to the 8 MiB budget; header probing never
> applies here because formatted sheets need the whole part.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%page%`, `%total%`, `%name%`, `%reason%`,
`%rows%`, `%cols%`, `%format%`, `%sheets%`, `%size%`). Only `en_us` and
`de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `xlsx.prev` | `Previous` | `Zurück` |
| `xlsx.next` | `Next` | `Weiter` |
| `xlsx.sheet` | `Sheet %page% of %total%: %name%` | `Blatt %page% von %total%: %name%` |
| `xlsx.load_failed` | `Cannot open this spreadsheet: %reason%` | `Diese Tabelle kann nicht geöffnet werden: %reason%` |
| `xlsx.truncated` | `Large sheet: showing the first %rows% rows and %cols% columns` | `Große Tabelle: nur die ersten %rows% Zeilen und %cols% Spalten werden gezeigt` |
| `xlsx.empty_sheet` | `This sheet contains no data.` | `Dieses Blatt enthält keine Daten.` |
| `status.xlsx` | `%format%, %sheets% sheets, %rows%, %size%, read-only` | `%format%, %sheets% Blätter, %rows%, %size%, schreibgeschützt` |

```json
{
  "xlsx.sheet": "Sheet %page% of %total%: %name%"
}
```

## Dependencies

Parsing uses `flate2` (raw deflate for method-8 ZIP entries) and
`quick-xml` (workbook XML), both already direct dependencies from the
document work (same `Cargo.lock` closure). Declared in `Cargo.toml`:

```toml
flate2 = "1.1"
quick-xml = "0.37"
```

No new dependency was added and no existing dependency version was
changed. No new system package is needed. The versions stay
user-managed: bump them only on request.

## Usage / Example

Open a spreadsheet from the command line or the native file dialog:

```bash
cargo run -- /path/to/table.xlsx
```

The first row renders bold, columns show letters and rows show
numbers, sheet tabs switch between sheets. There is no edit mode and
no save path for spreadsheets.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Spreadsheet);
let book = xlsx::load_workbook(path)?;
println!("{:?}", book.sheet_dims(0));
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
- [Pptx.md](Pptx.md) – read-only presentation slide viewer
