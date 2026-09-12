//! Read-only spreadsheet support for Preview.
//!
//! `.xlsx` files are ZIP packages holding `xl/workbook.xml` (sheet order),
//! `xl/_rels/workbook.xml.rels` (sheet targets), `xl/sharedStrings.xml`
//! (shared text) and `xl/worksheets/sheetN.xml` (grids). `.ods` files are
//! ZIP packages holding `content.xml` (OpenDocument with
//! `office:spreadsheet` tables). `.csv` and `.tsv` files are plain
//! delimited text parsed with pure logic (quotes, escapes and embedded
//! line breaks included). Legacy `.xls` (OLE compound) files have no
//! decoder here and report an honest hint. Parsing uses only crates
//! already in the `Cargo.lock` closure (`flate2` for deflated ZIP entries,
//! `quick-xml` for the workbook XML), so no new crate family is
//! introduced. Rendering targets a read-only `GtkGrid` with column
//! letters, row numbers and a bold first (header) row, reusing the SF Pro
//! Display styling from the other viewers. Spreadsheets are never editable
//! and never saved.

use std::collections::BTreeMap;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;

/// Max decompressed XML parsed per ZIP entry (same budget as text files).
const MAX_PART_BYTES: u64 = crate::model::MAX_TEXT_BYTES;

/// Max rows shown per sheet; longer sheets stop with `truncated` set.
pub const MAX_SHEET_ROWS: usize = 1000;

/// Max columns shown per sheet; wider sheets stop with `truncated` set.
pub const MAX_SHEET_COLS: usize = 100;

/// Max sheets parsed per workbook; larger workbooks stop with `truncated`.
pub const MAX_SHEETS: usize = 100;

/// One sheet: a display name plus a rectangular grid of plain-text cells
/// (ragged input rows are padded with empty strings).
#[derive(Clone, Debug)]
pub struct Sheet {
  /// Display name (workbook sheet name, ODF table name or file stem).
  pub name: String,
  /// Rectangular rows of plain-text cells (possibly empty).
  pub rows: Vec<Vec<String>>,
}

/// A loaded workbook: sheets in workbook order plus file metadata.
#[derive(Clone, Debug)]
pub struct Workbook {
  /// Short format label (`XLSX`, `XLS`, `CSV`, `TSV`, `ODS`).
  pub format: String,
  /// Sheets in workbook order.
  pub sheets: Vec<Sheet>,
  /// File size in bytes.
  pub file_bytes: u64,
  /// True when a cap stopped the parse (rows, columns or sheets omitted).
  pub truncated: bool,
}

impl Workbook {
  /// Number of sheets in the workbook.
  pub fn sheet_count(&self) -> usize {
    self.sheets.len()
  }

  /// Dimensions `(rows, cols)` of one sheet (0-based index). Returns
  /// `(0, 0)` for an out-of-range index or an empty sheet.
  pub fn sheet_dims(&self, index: usize) -> (usize, usize) {
    match self.sheets.get(index) {
      Some(sheet) if !sheet.rows.is_empty() => {
        let cols = sheet.rows.iter().map(|row| row.len()).max().unwrap_or(0);
        (sheet.rows.len(), cols)
      }
      _ => (0, 0),
    }
  }
}

/// Load a spreadsheet for read-only preview. Returns a human-readable
/// reason for missing files, directories, empty files, oversized files,
/// password-protected packages, legacy `.xls` files and corrupt content.
/// Never panics on corrupt input.
pub fn load_workbook(path: &Path) -> Result<Workbook, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if meta.len() == 0 {
    return Err("file is empty".to_string());
  }
  if meta.len() > crate::model::MAX_TEXT_BYTES {
    return Err("file is too large for the sheet viewer (over 8 MiB)".to_string());
  }
  let bytes = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  let format = crate::model::spreadsheet_format_label(&ext).to_string();
  if is_zip_magic(&bytes) {
    return load_zip_package(&bytes, format, meta.len());
  }
  if is_ole_magic(&bytes) {
    return Err(ole_hint(&ext));
  }
  if ext == "csv" || ext == "tsv" {
    return load_delimited(&bytes, &ext, path, format, meta.len());
  }
  if ext == "xlsx" || ext == "ods" {
    return Err("file is not a ZIP package (spreadsheet XML packages always start with PK)".to_string());
  }
  if ext == "xls" {
    return Err(
      "file is not a recognized spreadsheet (legacy .xls files are OLE compound files)"
        .to_string(),
    );
  }
  Err("file is not a recognized spreadsheet (not an xlsx, ods, csv, tsv or legacy xls file)"
    .to_string())
}

/// Hint for OLE compound files: password-protected OOXML packages use OLE
/// too, so `.xlsx`/`.ods` extensions report the password hint while `.xls`
/// (and unknown extensions) report the legacy hint.
fn ole_hint(ext: &str) -> String {
  if ext == "xlsx" || ext == "ods" {
    "file is encrypted (password-protected); open it without a password is not supported"
      .to_string()
  } else {
    "legacy .xls format is not parsed (this includes password-protected files); convert it to .xlsx to preview it"
      .to_string()
  }
}

fn is_zip_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04])
}

fn is_ole_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
}

/// Load a ZIP-based package: route to the `.xlsx` or `.ods` parser by its
/// entries, or report an honest hint for other archives (word documents,
/// presentations, plain zips).
fn load_zip_package(bytes: &[u8], format: String, file_bytes: u64) -> Result<Workbook, String> {
  let entries = central_dir(bytes)?;
  let has = |name: &str| entries.iter().any(|e| e.name == name);
  if has("xl/workbook.xml") {
    return load_xlsx(&entries, bytes, format, file_bytes);
  }
  if has("word/document.xml") {
    return Err("this is a word document, only spreadsheets (.xlsx) are supported".to_string());
  }
  if has("ppt/presentation.xml") {
    return Err("this is a presentation, only spreadsheets (.xlsx) are supported".to_string());
  }
  if has("content.xml") {
    let xml = extract(&entries, bytes, "content.xml")?;
    if contains_office_spreadsheet(&xml) {
      let (sheets, truncated) = parse_ods_xml(&xml);
      if sheets.is_empty() {
        return Err("spreadsheet has no readable sheets".to_string());
      }
      return Ok(Workbook { format, sheets, file_bytes, truncated });
    }
    if contains_office_text(&xml) {
      return Err("this is a word document, only spreadsheets (.xlsx) are supported".to_string());
    }
    if contains_draw_page(&xml) {
      return Err("this is a presentation, only spreadsheets (.xlsx) are supported".to_string());
    }
    return Err("ZIP archive holds no spreadsheet (missing xl/workbook.xml)".to_string());
  }
  Err("ZIP archive holds no spreadsheet (missing xl/workbook.xml)".to_string())
}

/// True when the ODF `content.xml` holds a spreadsheet body (checked on
/// the raw bytes so the XML never needs parsing for routing).
fn contains_office_spreadsheet(xml: &[u8]) -> bool {
  find_subslice(xml, b"office:spreadsheet").is_some()
}

/// True when the XML holds an OpenDocument text body (an ODT package
/// misnamed as a spreadsheet).
fn contains_office_text(xml: &[u8]) -> bool {
  find_subslice(xml, b"office:text").is_some()
}

/// True when the XML holds an OpenDocument draw page (an ODP package
/// misnamed as a spreadsheet).
fn contains_draw_page(xml: &[u8]) -> bool {
  find_subslice(xml, b"draw:page").is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  if needle.len() > haystack.len() {
    return None;
  }
  haystack.windows(needle.len()).position(|window| window == needle)
}

/// Load a `.csv`/`.tsv` file: decode as UTF-8 (BOM stripped, binary
/// rejected) and parse with the matching delimiter into a single sheet
/// named after the file stem.
fn load_delimited(
  bytes: &[u8],
  ext: &str,
  path: &Path,
  format: String,
  file_bytes: u64,
) -> Result<Workbook, String> {
  if bytes.contains(&0) {
    return Err("file looks like binary data".to_string());
  }
  let mut text =
    String::from_utf8(bytes.to_vec()).map_err(|_| "file is not valid UTF-8 text".to_string())?;
  if let Some(stripped) = text.strip_prefix('\u{FEFF}') {
    text = stripped.to_string();
  }
  let delim = if ext == "tsv" {
    '\t'
  } else {
    detect_csv_delim(&text)
  };
  let (rows, truncated) = parse_delimited_capped(&text, delim);
  let name = path
    .file_stem()
    .and_then(|stem| stem.to_str())
    .filter(|stem| !stem.is_empty())
    .map(|stem| stem.to_string())
    .unwrap_or_else(|| "Sheet1".to_string());
  Ok(Workbook {
    format,
    sheets: vec![Sheet { name, rows }],
    file_bytes,
    truncated,
  })
}

/// Pick the `.csv` delimiter from the first line: a comma wins, otherwise
/// a semicolon (the German Excel default) wins, otherwise comma. `.tsv`
/// files never reach this function (they always use tab).
fn detect_csv_delim(text: &str) -> char {
  let first = text.lines().next().unwrap_or("");
  if first.contains(',') {
    ','
  } else if first.contains(';') {
    ';'
  } else {
    ','
  }
}

/// Parse delimited text into a rectangular grid, capping rows and columns
/// at `MAX_SHEET_ROWS`/`MAX_SHEET_COLS` with the truncation flag set.
/// Ragged rows are padded with empty strings.
fn parse_delimited_capped(text: &str, delim: char) -> (Vec<Vec<String>>, bool) {
  let mut rows = parse_delimited(text, delim);
  let mut truncated = false;
  if rows.len() > MAX_SHEET_ROWS {
    rows.truncate(MAX_SHEET_ROWS);
    truncated = true;
  }
  let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
  let capped_width = width.min(MAX_SHEET_COLS);
  if width > MAX_SHEET_COLS {
    truncated = true;
  }
  for row in &mut rows {
    row.truncate(capped_width);
    while row.len() < capped_width {
      row.push(String::new());
    }
  }
  (rows, truncated)
}

/// Parse delimited text (`delim` separator) into rows of fields: quoted
/// fields may hold the delimiter, quotes (`""` escape) and embedded line
/// breaks (`\r\n`, `\n` and `\r` all end unquoted rows). A quote only
/// opens a quoted field at the start of a field; anywhere else it stays a
/// literal character. Lenient on corrupt input: an unclosed quote runs to
/// the end of the file instead of failing.
pub fn parse_delimited(text: &str, delim: char) -> Vec<Vec<String>> {
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut field = String::new();
  let mut chars = text.chars().peekable();
  let mut in_quotes = false;
  // True once a line break ended a row, so a trailing newline does not
  // produce a phantom empty row.
  let mut ended_with_break = false;
  while let Some(c) = chars.next() {
    ended_with_break = false;
    if in_quotes {
      if c == '"' {
        if chars.peek() == Some(&'"') {
          chars.next();
          field.push('"');
        } else {
          in_quotes = false;
        }
      } else {
        field.push(c);
      }
      continue;
    }
    if c == '"' && field.is_empty() {
      in_quotes = true;
    } else if c == delim {
      row.push(std::mem::take(&mut field));
    } else if c == '\r' {
      if chars.peek() == Some(&'\n') {
        chars.next();
      }
      row.push(std::mem::take(&mut field));
      rows.push(std::mem::take(&mut row));
      ended_with_break = true;
    } else if c == '\n' {
      row.push(std::mem::take(&mut field));
      rows.push(std::mem::take(&mut row));
      ended_with_break = true;
    } else {
      field.push(c);
    }
  }
  if in_quotes || !field.is_empty() || !row.is_empty() {
    // A file ending in a line break already pushed its last row; only the
    // leftover partial row is flushed here.
    if !field.is_empty() || !row.is_empty() || !ended_with_break {
      row.push(std::mem::take(&mut field));
      rows.push(row);
    }
  }
  rows
}

/// Load an `.xlsx` package: sheet order comes from `xl/workbook.xml` via
/// the relationship targets in `xl/_rels/workbook.xml.rels`; when the
/// relationships part is missing, `xl/worksheets/sheet*.xml` entries are
/// used in sorted name order instead.
fn load_xlsx(
  entries: &[ZipEntry],
  bytes: &[u8],
  format: String,
  file_bytes: u64,
) -> Result<Workbook, String> {
  let order_xml = extract(entries, bytes, "xl/workbook.xml")?;
  let rels = extract_optional(entries, bytes, "xl/_rels/workbook.xml.rels");
  let shared_xml = extract_optional(entries, bytes, "xl/sharedStrings.xml");
  let shared = shared_xml
    .as_deref()
    .map(parse_shared_strings)
    .unwrap_or_default();
  let mut order = sheet_order(&order_xml, rels.as_deref(), entries);
  if order.is_empty() {
    return Err("spreadsheet has no readable sheets".to_string());
  }
  let mut truncated = false;
  if order.len() > MAX_SHEETS {
    order.truncate(MAX_SHEETS);
    truncated = true;
  }
  let mut sheets = Vec::with_capacity(order.len());
  for (name, entry) in &order {
    let xml = extract(entries, bytes, entry)?;
    let (rows, sheet_capped) = parse_sheet_xml(&xml, &shared);
    if sheet_capped {
      truncated = true;
    }
    sheets.push(Sheet { name: name.clone(), rows });
  }
  Ok(Workbook { format, sheets, file_bytes, truncated })
}

/// Resolve the sheet order: sheet names plus relationship ids from the
/// workbook part mapped through the relationships part to `xl/...` entry
/// names. Falls back to sorted `xl/worksheets/sheet*.xml` entry names
/// (named after their file stem) when the relationships part is missing
/// or names no usable target.
fn sheet_order(
  order_xml: &[u8],
  rels: Option<&[u8]>,
  entries: &[ZipEntry],
) -> Vec<(String, String)> {
  let sheets = workbook_sheets(order_xml);
  if !sheets.is_empty() {
    if let Some(rels_xml) = rels {
      let targets = rel_targets(rels_xml);
      let mut order = Vec::new();
      for (name, rid) in &sheets {
        if let Some(target) = targets.get(rid) {
          let entry = rel_target_name(target);
          if entries.iter().any(|e| e.name == entry) {
            order.push((name.clone(), entry));
          }
        }
      }
      if !order.is_empty() {
        return order;
      }
    }
  }
  let mut fallback: Vec<String> = entries
    .iter()
    .map(|e| e.name.clone())
    .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
    .collect();
  fallback.sort();
  fallback
    .into_iter()
    .map(|entry| {
      let stem = entry
        .rsplit('/')
        .next()
        .and_then(|base| base.strip_suffix(".xml"))
        .unwrap_or("Sheet");
      (stem.to_string(), entry)
    })
    .collect()
}

/// Collect sheet names plus relationship ids in workbook order. Nameless
/// sheets get a `Sheet N` fallback name in encounter order.
fn workbook_sheets(xml: &[u8]) -> Vec<(String, String)> {
  let mut reader = Reader::from_reader(xml);
  let mut sheets = Vec::new();
  let mut counter = 0usize;
  loop {
    match reader.read_event() {
      Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
        if local(e.name().into_inner()) == b"sheet" {
          counter += 1;
          let name = attr_value(e, b"name").unwrap_or_else(|| format!("Sheet {counter}"));
          if let Some(rid) = attr_value(e, b"id") {
            sheets.push((name, rid));
          } else {
            // Sheets without a relationship id cannot be resolved to a
            // part; they are skipped (the sorted fallback only runs when
            // no sheet resolves at all).
            let _ = name;
          }
        }
      }
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
  }
  sheets
}

/// Map relationship `Id` to `Target` for sheet relationships.
fn rel_targets(xml: &[u8]) -> std::collections::HashMap<String, String> {
  let mut reader = Reader::from_reader(xml);
  let mut map = std::collections::HashMap::new();
  loop {
    match reader.read_event() {
      Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
        if local(e.name().into_inner()) == b"Relationship" {
          if let (Some(id), Some(target)) = (attr_value(e, b"Id"), attr_value(e, b"Target")) {
            map.insert(id, target);
          }
        }
      }
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
  }
  map
}

/// Turn a relationship target (`worksheets/sheet1.xml`, `/xl/...` or an
/// absolute URI) into a ZIP entry name. External targets (http, mailto,
/// custom protocols) resolve to an empty name that never matches an entry.
fn rel_target_name(target: &str) -> String {
  if target.contains("://") || target.starts_with("mailto:") {
    return String::new();
  }
  let trimmed = target.trim_start_matches('/');
  if trimmed.starts_with("xl/") {
    trimmed.to_string()
  } else {
    format!("xl/{trimmed}")
  }
}

/// Parse `xl/sharedStrings.xml` into the shared string table: each `si`
/// contributes its concatenated `t` text (runs and phonetics included in
/// encounter order).
fn parse_shared_strings(xml: &[u8]) -> Vec<String> {
  let mut reader = Reader::from_reader(xml);
  let mut table = Vec::new();
  let mut in_item = false;
  let mut in_text_elem = false;
  let mut current = String::new();
  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        match local(e.name().into_inner()) {
          b"si" => {
            in_item = true;
            current.clear();
          }
          b"t" => {
            if in_item {
              in_text_elem = true;
            }
          }
          _ => {}
        }
      }
      Ok(Event::End(ref e)) => {
        match local(e.name().into_inner()) {
          b"si" => {
            in_item = false;
            table.push(std::mem::take(&mut current));
          }
          b"t" => in_text_elem = false,
          _ => {}
        }
      }
      Ok(Event::Text(ref e)) => {
        if in_item && in_text_elem {
          current.push_str(&text_content(e));
        }
      }
      Ok(Event::CData(ref e)) => {
        if in_item && in_text_elem {
          current.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }
  table
}

/// Parse one `xl/worksheets/sheetN.xml` part into a rectangular grid of
/// plain-text cells: shared strings (`t="s"`) resolve through the table,
/// inline strings (`t="inlineStr"`) read their `is/t` text, booleans
/// (`t="b"`) render as `TRUE`/`FALSE`, formula results (`t="str"`) and
/// errors (`t="e"`) use their `v` text and numbers use their raw `v`
/// text (serial dates and number formats are not evaluated). Cells are
/// placed by their `r` reference (`B2`); cells without a reference use
/// running counters. Returns the grid plus true when the row/column cap
/// omitted data.
fn parse_sheet_xml(xml: &[u8], shared: &[String]) -> (Vec<Vec<String>>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut sparse: BTreeMap<(usize, usize), String> = BTreeMap::new();
  let mut next_row: usize = 0;
  let mut cur_row: usize = 0;
  let mut col_next: usize = 0;
  let mut cell_row: usize = 0;
  let mut cell_col: usize = 0;
  let mut cell_kind = String::new();
  let mut cell_value = String::new();
  let mut in_cell = false;
  // True inside `v` or inside `is/t`: only there is cell character data.
  let mut in_value_elem = false;

  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        match local(e.name().into_inner()) {
          b"row" => {
            let idx = attr_value(e, b"r")
              .and_then(|v| v.parse::<usize>().ok())
              .filter(|v| *v > 0)
              .map(|v| v - 1)
              .unwrap_or(next_row);
            cur_row = idx;
            next_row = idx.saturating_add(1);
            col_next = 0;
          }
          b"c" => {
            let (row, col) = attr_value(e, b"r")
              .and_then(|r| parse_cell_ref(&r))
              .unwrap_or((cur_row, col_next));
            cell_row = row;
            cell_col = col;
            cell_kind = attr_value(e, b"t").unwrap_or_default();
            cell_value.clear();
            in_cell = true;
          }
          b"v" => {
            if in_cell {
              in_value_elem = true;
            }
          }
          b"is" => {
            // Inline string container; its `t` children hold the text.
          }
          b"t" => {
            if in_cell {
              in_value_elem = true;
            }
          }
          _ => {}
        }
      }
      Ok(Event::Empty(ref e)) => {
        // Empty cells (`<c r="B2"/>`) carry no value but still advance
        // the running column counter when they name a reference.
        if local(e.name().into_inner()) == b"c" {
          if let Some((_, col)) =
            attr_value(e, b"r").as_deref().and_then(parse_cell_ref)
          {
            if col_next <= col {
              col_next = col.saturating_add(1);
            }
          }
        }
      }
      Ok(Event::End(ref e)) => {
        match local(e.name().into_inner()) {
          b"c" => {
            if in_cell {
              in_cell = false;
              let text = resolve_cell(&cell_kind, &cell_value, shared);
              if !text.is_empty() {
                sparse.insert((cell_row, cell_col), text);
              }
              if col_next <= cell_col {
                col_next = cell_col.saturating_add(1);
              }
            }
          }
          b"v" | b"t" => in_value_elem = false,
          _ => {}
        }
      }
      Ok(Event::Text(ref e)) => {
        if in_cell && in_value_elem {
          cell_value.push_str(&text_content(e));
        }
      }
      Ok(Event::CData(ref e)) => {
        if in_cell && in_value_elem {
          cell_value.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }

  let max_row = sparse.keys().map(|(r, _)| *r).max();
  let max_col = sparse.keys().map(|(_, c)| *c).max();
  let Some(last_row) = max_row else {
    return (Vec::new(), false);
  };
  let last_col = max_col.unwrap_or(0);
  let mut truncated = false;
  let rows = (last_row + 1).min(MAX_SHEET_ROWS);
  let cols = (last_col + 1).min(MAX_SHEET_COLS);
  if last_row + 1 > MAX_SHEET_ROWS || last_col + 1 > MAX_SHEET_COLS {
    truncated = true;
  }
  let mut grid = vec![vec![String::new(); cols]; rows];
  for ((r, c), text) in sparse {
    if r < rows && c < cols {
      grid[r][c] = text;
    }
  }
  (grid, truncated)
}

/// Resolve one cell value by its type flag (`t` attribute): shared string
/// index, inline string text, boolean, or raw `v` text (numbers, formula
/// results and errors alike). Returns an empty string for missing or
/// unresolvable values so the caller skips the cell.
fn resolve_cell(kind: &str, value: &str, shared: &[String]) -> String {
  match kind {
    "s" => value
      .parse::<usize>()
      .ok()
      .and_then(|idx| shared.get(idx))
      .cloned()
      .unwrap_or_default(),
    "inlineStr" => value.to_string(),
    "b" => {
      if value.is_empty() {
        String::new()
      } else if value == "1" || value.eq_ignore_ascii_case("true") {
        "TRUE".to_string()
      } else {
        "FALSE".to_string()
      }
    }
    _ => value.to_string(),
  }
}

/// Parse `content.xml` of an ODS package into sheets in `table:table`
/// order: repeated rows (`number-rows-repeated`) and repeated cells
/// (`number-columns-repeated`) expand in place, cell text comes from the
/// `text:p` paragraphs (whitespace-normalized) with `office:value`,
/// `office:boolean-value` and `office:date-value` as fallback when a cell
/// holds no text. Covered cells count as empty cells. Only outermost
/// tables build sheets; nested tables are ignored. Returns the sheets
/// plus the truncation flag.
fn parse_ods_xml(xml: &[u8]) -> (Vec<Sheet>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut sheets: Vec<Sheet> = Vec::new();
  let mut truncated = false;
  let mut table_depth: usize = 0;
  let mut name = String::new();
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut row_repeat: usize = 1;
  let mut col_repeat: usize = 1;
  let mut cell_texts: Vec<String> = Vec::new();
  let mut cell_value = String::new();
  let mut cell_boolean = String::new();
  let mut cell_date = String::new();
  let mut in_para = false;
  let mut para = String::new();

  macro_rules! push_sheet {
    () => {
      if sheets.len() >= MAX_SHEETS {
        truncated = true;
      } else {
        pad_grid(&mut rows);
        sheets.push(Sheet { name: std::mem::take(&mut name), rows: std::mem::take(&mut rows) });
      }
    };
  }

  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        match local(e.name().into_inner()) {
          b"table" => {
            if table_depth == 0 {
              let fallback = format!("Sheet {}", sheets.len() + 1);
              name = attr_value(e, b"name").unwrap_or(fallback);
              // Skip empty fallback names.
              if name.trim().is_empty() {
                name = format!("Sheet {}", sheets.len() + 1);
              }
              rows.clear();
            }
            table_depth += 1;
          }
          b"table-row" => {
            if table_depth == 1 {
              row_repeat = attr_value(e, b"number-rows-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_SHEET_ROWS);
              row.clear();
            }
          }
          b"table-cell" | b"covered-table-cell" => {
            if table_depth == 1 {
              col_repeat = attr_value(e, b"number-columns-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_SHEET_COLS);
              cell_texts.clear();
              cell_value = attr_value(e, b"value").unwrap_or_default();
              cell_boolean = attr_value(e, b"boolean-value").unwrap_or_default();
              cell_date = attr_value(e, b"date-value").unwrap_or_default();
            }
          }
          b"p" => {
            if table_depth == 1 {
              in_para = true;
              para.clear();
            }
          }
          _ => {}
        }
      }
      Ok(Event::Empty(ref e)) => {
        match local(e.name().into_inner()) {
          b"s" => {
            if in_para && table_depth == 1 {
              let count = attr_value(e, b"c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
              for _ in 0..count.min(64) {
                para.push(' ');
              }
            }
          }
          b"line-break" => {
            if in_para && table_depth == 1 {
              para.push('\n');
            }
          }
          // Self-closing cells (`<table:table-cell .../>`) hold no text
          // children, so they resolve straight to their attribute
          // fallback (or an empty cell) instead of waiting for an end
          // tag that never comes.
          b"table-cell" | b"covered-table-cell" => {
            if table_depth == 1 {
              let repeat = attr_value(e, b"number-columns-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_SHEET_COLS);
              let text = ods_fallback(
                &attr_value(e, b"boolean-value").unwrap_or_default(),
                &attr_value(e, b"value").unwrap_or_default(),
                &attr_value(e, b"date-value").unwrap_or_default(),
              );
              for _ in 0..repeat {
                if row.len() >= MAX_SHEET_COLS {
                  truncated = true;
                  break;
                }
                row.push(text.clone());
              }
            }
          }
          // A self-closing row is an empty row (repeated as requested).
          b"table-row" => {
            if table_depth == 1 {
              let repeat = attr_value(e, b"number-rows-repeated")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, MAX_SHEET_ROWS);
              for _ in 0..repeat {
                if rows.len() >= MAX_SHEET_ROWS {
                  truncated = true;
                  break;
                }
                rows.push(Vec::new());
              }
            }
          }
          _ => {}
        }
      }
      Ok(Event::End(ref e)) => {
        match local(e.name().into_inner()) {
          b"table" => {
            table_depth = table_depth.saturating_sub(1);
            if table_depth == 0 {
              push_sheet!();
            }
          }
          b"p" => {
            if in_para && table_depth == 1 {
              in_para = false;
              let line = para.split_whitespace().collect::<Vec<_>>().join(" ");
              if !line.is_empty() {
                cell_texts.push(line);
              }
            }
          }
          b"table-cell" | b"covered-table-cell" => {
            if table_depth == 1 {
              let text = if cell_texts.is_empty() {
                ods_fallback(&cell_boolean, &cell_value, &cell_date)
              } else {
                cell_texts.join(" ")
              };
              for _ in 0..col_repeat {
                if row.len() >= MAX_SHEET_COLS {
                  truncated = true;
                  break;
                }
                row.push(text.clone());
              }
            }
          }
          b"table-row" => {
            if table_depth == 1 {
              for _ in 0..row_repeat {
                if rows.len() >= MAX_SHEET_ROWS {
                  truncated = true;
                  break;
                }
                rows.push(row.clone());
              }
            }
          }
          _ => {}
        }
      }
      Ok(Event::Text(ref e)) => {
        if in_para && table_depth == 1 {
          // Inter-tag pretty-print whitespace is junk (real spaces use
          // `text:s`); all other spacing is kept verbatim so words around
          // spans do not glue together.
          let text = text_content(e);
          if !(text.trim().is_empty() && (text.contains('\n') || text.contains('\r'))) {
            para.push_str(&text);
          }
        }
      }
      Ok(Event::CData(ref e)) => {
        if in_para && table_depth == 1 {
          para.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }
  (sheets, truncated)
}

/// Fallback cell text for ODS cells without `text:p` content: boolean,
/// numeric value or date value attributes in that order.
fn ods_fallback(boolean: &str, value: &str, date: &str) -> String {
  if !boolean.is_empty() {
    if boolean.eq_ignore_ascii_case("true") || boolean == "1" {
      return "TRUE".to_string();
    }
    return "FALSE".to_string();
  }
  if !value.is_empty() {
    return value.to_string();
  }
  date.to_string()
}

/// Pad ragged rows with empty strings so the grid is rectangular.
fn pad_grid(rows: &mut [Vec<String>]) {
  let width = rows.iter().map(|row| row.len()).max().unwrap_or(0);
  for row in rows {
    while row.len() < width {
      row.push(String::new());
    }
  }
}

/// Column letters for a 0-based column index (`0` is `A`, `25` is `Z`,
/// `26` is `AA`).
pub fn col_letters(mut col: usize) -> String {
  let mut out = Vec::new();
  loop {
    out.push((b'A' + (col % 26) as u8) as char);
    if col < 26 {
      break;
    }
    col = col / 26 - 1;
  }
  out.iter().rev().collect()
}

/// Parse a cell reference (`B2`, absolute `$B$2` included) into a 0-based
/// `(row, col)` pair. Returns `None` for malformed references and row 0.
pub fn parse_cell_ref(cell_ref: &str) -> Option<(usize, usize)> {
  let cleaned: String = cell_ref.chars().filter(|c| *c != '$').collect();
  let split = cleaned.find(|c: char| !c.is_ascii_alphabetic())?;
  let (letters, digits) = cleaned.split_at(split);
  if letters.is_empty() || digits.is_empty() {
    return None;
  }
  if !digits.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let mut col = 0usize;
  for byte in letters.bytes() {
    if !byte.is_ascii_alphabetic() {
      return None;
    }
    let digit = (byte.to_ascii_uppercase() - b'A') as usize + 1;
    col = col.checked_mul(26)?.checked_add(digit)?;
  }
  let col = col.checked_sub(1)?;
  let row: usize = digits.parse().ok()?;
  if row == 0 {
    return None;
  }
  Some((row - 1, col))
}

/// True when the cell text is numeric (rendered right-aligned in the
/// grid). Empty strings never count as numbers.
pub fn is_number_text(text: &str) -> bool {
  !text.is_empty() && text.parse::<f64>().is_ok()
}

/// One central-directory entry of a ZIP archive.
struct ZipEntry {
  name: String,
  method: u16,
  flags: u16,
  comp_size: u64,
  local_offset: u64,
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
  bytes.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
  bytes
    .get(at..at + 4)
    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Parse the central directory via the end-of-central-directory record.
/// Only stored (method 0) and deflated (method 8) entries can be extracted;
/// anything else reports an honest error at extract time.
fn central_dir(bytes: &[u8]) -> Result<Vec<ZipEntry>, String> {
  const EOCD: [u8; 4] = [0x50, 0x4B, 0x05, 0x06];
  if bytes.len() < 22 {
    return Err("file is not a ZIP package".to_string());
  }
  let search_start = bytes.len().saturating_sub(65_557 + 22);
  let mut eocd = None;
  let mut i = bytes.len() - 22;
  loop {
    if bytes[i..].starts_with(&EOCD) {
      eocd = Some(i);
      break;
    }
    if i == search_start {
      break;
    }
    i -= 1;
  }
  let eocd = eocd.ok_or_else(|| "file is not a ZIP package".to_string())?;
  let count = read_u16(bytes, eocd + 10).ok_or_else(|| "ZIP directory is truncated".to_string())?;
  let cd_offset = read_u32(bytes, eocd + 16).ok_or_else(|| "ZIP directory is truncated".to_string())?;
  let mut entries = Vec::new();
  let mut at = cd_offset as usize;
  for _ in 0..count {
    if bytes.get(at..at + 4) != Some(&[0x50, 0x4B, 0x01, 0x02][..]) {
      return Err("ZIP directory is corrupt".to_string());
    }
    let flags = read_u16(bytes, at + 8).ok_or_else(|| "ZIP directory is corrupt".to_string())?;
    let method = read_u16(bytes, at + 10).ok_or_else(|| "ZIP directory is corrupt".to_string())?;
    let comp_size =
      read_u32(bytes, at + 20).ok_or_else(|| "ZIP directory is corrupt".to_string())? as u64;
    let name_len =
      read_u16(bytes, at + 28).ok_or_else(|| "ZIP directory is corrupt".to_string())? as usize;
    let extra_len =
      read_u16(bytes, at + 30).ok_or_else(|| "ZIP directory is corrupt".to_string())? as usize;
    let comment_len =
      read_u16(bytes, at + 32).ok_or_else(|| "ZIP directory is corrupt".to_string())? as usize;
    let local_offset =
      read_u32(bytes, at + 42).ok_or_else(|| "ZIP directory is corrupt".to_string())? as u64;
    let name_start = at + 46;
    let name_end = name_start.saturating_add(name_len);
    let name_bytes = bytes.get(name_start..name_end).ok_or_else(|| "ZIP directory is corrupt".to_string())?;
    entries.push(ZipEntry {
      name: String::from_utf8_lossy(name_bytes).into_owned(),
      method,
      flags,
      comp_size,
      local_offset,
    });
    at = name_end.saturating_add(extra_len).saturating_add(comment_len);
  }
  Ok(entries)
}

/// Extract one entry by name with a decompression cap. Password-protected
/// entries report the password hint instead of garbage.
fn extract(entries: &[ZipEntry], bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
  let entry = entries
    .iter()
    .find(|e| e.name == name)
    .ok_or_else(|| format!("archive holds no {name}"))?;
  if entry.flags & 0x1 != 0 {
    return Err(
      "file is encrypted (password-protected); open it without a password is not supported"
        .to_string(),
    );
  }
  inflate_entry(bytes, entry)
}

/// Extract one optional entry: `None` when the entry is missing, otherwise
/// the same errors as `extract` (corrupt entries still fail the workbook).
fn extract_optional(entries: &[ZipEntry], bytes: &[u8], name: &str) -> Option<Vec<u8>> {
  if entries.iter().any(|e| e.name == name) {
    extract(entries, bytes, name).ok()
  } else {
    None
  }
}

fn inflate_entry(bytes: &[u8], entry: &ZipEntry) -> Result<Vec<u8>, String> {
  let off = entry.local_offset as usize;
  if bytes.get(off..off + 4) != Some(&[0x50, 0x4B, 0x03, 0x04][..]) {
    return Err("ZIP entry header is corrupt".to_string());
  }
  let name_len = read_u16(bytes, off + 26).ok_or_else(|| "ZIP entry is corrupt".to_string())? as usize;
  let extra_len = read_u16(bytes, off + 28).ok_or_else(|| "ZIP entry is corrupt".to_string())? as usize;
  let data_off = off.saturating_add(30).saturating_add(name_len).saturating_add(extra_len);
  let comp_size = entry.comp_size.min(bytes.len() as u64) as usize;
  let comp =
    bytes.get(data_off..data_off.saturating_add(comp_size)).ok_or_else(|| {
      "ZIP entry is truncated".to_string()
    })?;
  // The central size wins over the slice clamp above: a short slice means
  // the archive is truncated.
  if comp.len() as u64 != entry.comp_size {
    return Err("ZIP entry is truncated".to_string());
  }
  match entry.method {
    0 => {
      if entry.comp_size > MAX_PART_BYTES {
        return Err("spreadsheet part is too large for the viewer (over 8 MiB)".to_string());
      }
      Ok(comp.to_vec())
    }
    8 => inflate_capped(comp),
    method => Err(format!("archive uses unsupported compression (method {method})")),
  }
}

/// Raw deflate decompression with a cap so hostile archives cannot expand
/// without bound.
fn inflate_capped(comp: &[u8]) -> Result<Vec<u8>, String> {
  use std::io::Read;
  let decoder = flate2::read::DeflateDecoder::new(comp);
  let mut out = Vec::new();
  decoder
    .take(MAX_PART_BYTES + 1)
    .read_to_end(&mut out)
    .map_err(|e| format!("cannot decompress spreadsheet part: {e}"))?;
  if out.len() as u64 > MAX_PART_BYTES {
    return Err("spreadsheet part is too large for the viewer (over 8 MiB)".to_string());
  }
  Ok(out)
}

/// Strip a namespace prefix (`x:c` becomes `c`).
fn local(raw: &[u8]) -> &[u8] {
  match raw.iter().rposition(|&b| b == b':') {
    Some(pos) => &raw[pos + 1..],
    None => raw,
  }
}

/// Read an attribute value by local name from a start/empty tag.
fn attr_value(tag: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
  for attr in tag.attributes() {
    let attr = attr.ok()?;
    if local(attr.key.into_inner()) == name {
      return Some(String::from_utf8_lossy(&attr.value).into_owned());
    }
  }
  None
}

/// Decoded text content of a text event (entities unescaped, lossy fallback).
fn text_content(event: &quick_xml::events::BytesText<'_>) -> String {
  event.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| {
    String::from_utf8_lossy(event).into_owned()
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Minimal ZIP writer for tests: stored entries only, matching the
  /// reader above (local headers, central directory, EOCD).
  fn zip_store(entries: &[(&str, &[u8])]) -> Vec<u8> {
    fn u16le(v: usize) -> [u8; 2] {
      (v as u16).to_le_bytes()
    }
    fn u32le(v: usize) -> [u8; 4] {
      (v as u32).to_le_bytes()
    }
    let mut buf = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
      let offset = buf.len();
      buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
      buf.extend_from_slice(&[0x14, 0x00, 0x00, 0x00, 0x00, 0x00]);
      buf.extend_from_slice(&[0x00, 0x00, 0x21, 0x00]);
      buf.extend_from_slice(&u32le(data.len()));
      buf.extend_from_slice(&u32le(data.len()));
      buf.extend_from_slice(&u32le(data.len()));
      buf.extend_from_slice(&u16le(name.len()));
      buf.extend_from_slice(&[0x00, 0x00]);
      buf.extend_from_slice(name.as_bytes());
      buf.extend_from_slice(data);
      central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
      central.extend_from_slice(&[0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00]);
      central.extend_from_slice(&[0x00, 0x00, 0x21, 0x00]);
      central.extend_from_slice(&u32le(data.len()));
      central.extend_from_slice(&u32le(data.len()));
      central.extend_from_slice(&u32le(data.len()));
      central.extend_from_slice(&u16le(name.len()));
      central.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
      central.extend_from_slice(&u32le(offset));
      central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = buf.len();
    buf.extend_from_slice(&central);
    let cd_size = buf.len() - cd_offset;
    buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    buf.extend_from_slice(&u16le(entries.len()));
    buf.extend_from_slice(&u16le(entries.len()));
    buf.extend_from_slice(&u32le(cd_size));
    buf.extend_from_slice(&u32le(cd_offset));
    buf.extend_from_slice(&[0x00, 0x00]);
    buf
  }

  fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("preview-xlsx-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write test spreadsheet");
    path
  }

  // Workbook order names Totals first even though Data sorts first.
  const SAMPLE_WORKBOOK: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:workbook xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<x:sheets>
<x:sheet name="Totals" r:id="rId2"/>
<x:sheet name="Data" r:id="rId1"/>
</x:sheets>
</x:workbook>"#;

  const SAMPLE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="sheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="sheet" Target="worksheets/sheet2.xml"/>
</Relationships>"#;

  const SAMPLE_SHARED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:sst xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="2">
<x:si><x:t>Name</x:t></x:si>
<x:si><x:r><x:t>Al</x:t></x:r><x:r><x:t>ice &amp; Bob</x:t></x:r></x:si>
</x:sst>"#;

  const SAMPLE_SHEET1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:worksheet xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<x:sheetData>
<x:row r="1"><x:c r="A1" t="s"><x:v>0</x:v></x:c><x:c r="B1" t="s"><x:v>1</x:v></x:c></x:row>
<x:row r="2"><x:c r="A2"><x:v>42</x:v></x:c><x:c r="B2" t="b"><x:v>1</x:v></x:c><x:c r="C2" t="inlineStr"><x:is><x:t>hi</x:t></x:is></x:c></x:row>
<x:row r="3"><x:c r="A3" t="str"><x:v>=SUM(A2)</x:v></x:c><x:c r="B3" t="e"><x:v>#DIV/0!</x:v></x:c><x:c r="D3"><x:v>7</x:v></x:c></x:row>
</x:sheetData>
</x:worksheet>"#;

  const SAMPLE_SHEET2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<x:worksheet xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<x:sheetData>
<x:row><x:c><x:v>1</x:v></x:c><x:c><x:v>2</x:v></x:c></x:row>
</x:sheetData>
</x:worksheet>"#;

  const SAMPLE_ODS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
<office:body><office:spreadsheet>
<table:table table:name="Prices">
<table:table-row>
<table:table-cell><text:p>Item</text:p></table:table-cell>
<table:table-cell table:number-columns-repeated="2"><text:p>9.5</text:p></table:table-cell>
</table:table-row>
<table:table-row table:number-rows-repeated="2">
<table:table-cell office:value-type="float" office:value="3"><text:p>3</text:p></table:table-cell>
<table:table-cell office:value-type="boolean" office:boolean-value="true"/>
<table:table-cell office:value-type="float" office:value="4.25"/>
</table:table-row>
</table:table>
</office:spreadsheet></office:body></office:document-content>"#;

  fn sample_xlsx_bytes() -> Vec<u8> {
    zip_store(&[
      ("[Content_Types].xml", b"<Types/>"),
      ("xl/workbook.xml", SAMPLE_WORKBOOK.as_bytes()),
      ("xl/_rels/workbook.xml.rels", SAMPLE_RELS.as_bytes()),
      ("xl/sharedStrings.xml", SAMPLE_SHARED.as_bytes()),
      ("xl/worksheets/sheet1.xml", SAMPLE_SHEET1.as_bytes()),
      ("xl/worksheets/sheet2.xml", SAMPLE_SHEET2.as_bytes()),
    ])
  }

  #[test]
  fn xlsx_sample_loads_in_workbook_order() {
    let path = write_temp("sample.xlsx", &sample_xlsx_bytes());
    let book = load_workbook(&path).expect("sample xlsx loads");
    assert_eq!(book.format, "XLSX");
    assert!(!book.truncated);
    assert_eq!(book.sheet_count(), 2);
    // Workbook order: Totals (sheet2) first, Data (sheet1) second.
    assert_eq!(book.sheets[0].name, "Totals");
    assert_eq!(book.sheets[1].name, "Data");
    let data = &book.sheets[1].rows;
    assert_eq!(data.len(), 3);
    assert_eq!(data[0], vec!["Name".to_string(), "Alice & Bob".to_string(), String::new(), String::new()]);
    assert_eq!(data[1][0], "42");
    assert_eq!(data[1][1], "TRUE");
    assert_eq!(data[1][2], "hi");
    assert_eq!(data[2][0], "=SUM(A2)");
    assert_eq!(data[2][1], "#DIV/0!");
    assert_eq!(data[2][3], "7");
    // Sparse cell C1 stays empty but the grid is rectangular.
    assert_eq!(data[0].len(), 4);
    // Sheet without references uses running counters.
    assert_eq!(book.sheets[0].rows, vec![vec!["1".to_string(), "2".to_string()]]);
    assert_eq!(book.sheet_dims(0), (1, 2));
    assert_eq!(book.sheet_dims(1), (3, 4));
    assert_eq!(book.sheet_dims(9), (0, 0));
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn xlsx_falls_back_to_sorted_sheets_without_rels() {
    let zip = zip_store(&[
      ("xl/workbook.xml", SAMPLE_WORKBOOK.as_bytes()),
      ("xl/worksheets/sheet2.xml", SAMPLE_SHEET2.as_bytes()),
      ("xl/worksheets/sheet1.xml", SAMPLE_SHEET1.as_bytes()),
    ]);
    let path = write_temp("norels.xlsx", &zip);
    let book = load_workbook(&path).expect("fallback order loads");
    assert_eq!(book.sheet_count(), 2);
    assert_eq!(book.sheets[0].name, "sheet1");
    assert_eq!(book.sheets[1].name, "sheet2");
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn ods_sample_parses_with_repeats() {
    let zip = zip_store(&[
      ("mimetype", b"application/vnd.oasis.opendocument.spreadsheet"),
      ("content.xml", SAMPLE_ODS.as_bytes()),
    ]);
    let path = write_temp("sample.ods", &zip);
    let book = load_workbook(&path).expect("sample ods loads");
    assert_eq!(book.format, "ODS");
    assert!(!book.truncated);
    assert_eq!(book.sheet_count(), 1);
    assert_eq!(book.sheets[0].name, "Prices");
    let rows = &book.sheets[0].rows;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0], vec!["Item".to_string(), "9.5".to_string(), "9.5".to_string()]);
    assert_eq!(rows[1], vec!["3".to_string(), "TRUE".to_string(), "4.25".to_string()]);
    assert_eq!(rows[2], rows[1]);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn csv_sample_parses_with_quotes() {
    let csv = "name,note,amount\n\"Doe, Jane\",\"say \"\"hi\"\"\",12.5\nplain,line with\nbreak,7\n\"multi\",\"line1\nline2\",end\n";
    let path = write_temp("sample.csv", csv.as_bytes());
    let book = load_workbook(&path).expect("sample csv loads");
    assert_eq!(book.format, "CSV");
    assert!(!book.truncated);
    assert_eq!(book.sheet_count(), 1);
    assert_eq!(book.sheets[0].name, "sample");
    let rows = &book.sheets[0].rows;
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0], vec!["name".to_string(), "note".to_string(), "amount".to_string()]);
    assert_eq!(rows[1][0], "Doe, Jane");
    assert_eq!(rows[1][1], "say \"hi\"");
    assert_eq!(rows[1][2], "12.5");
    // Plain line breaks end rows even mid-table.
    assert_eq!(rows[2], vec!["plain".to_string(), "line with".to_string(), String::new()]);
    assert_eq!(rows[3], vec!["break".to_string(), "7".to_string(), String::new()]);
    // A quoted line break stays inside its field.
    assert_eq!(rows[4], vec!["multi".to_string(), "line1\nline2".to_string(), "end".to_string()]);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn tsv_and_semicolon_csv_parse() {
    let tsv = "a\tb\n1\t2\n";
    let path = write_temp("sample.tsv", tsv.as_bytes());
    let book = load_workbook(&path).expect("sample tsv loads");
    assert_eq!(book.format, "TSV");
    assert_eq!(book.sheets[0].rows, vec![vec!["a".to_string(), "b".to_string()], vec!["1".to_string(), "2".to_string()]]);
    let _ = std::fs::remove_file(&path);
    // German Excel default: semicolons without any comma.
    let semi = "a;b\n1;2\n";
    let path = write_temp("german.csv", semi.as_bytes());
    let book = load_workbook(&path).expect("semicolon csv loads");
    assert_eq!(book.sheets[0].rows[1], vec!["1".to_string(), "2".to_string()]);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn corrupt_spreadsheets_hint_without_crash() {
    // Not a ZIP, delimited or OLE file.
    let plain = write_temp("bad.xlsx", b"just some text");
    assert!(load_workbook(&plain).is_err());
    let _ = std::fs::remove_file(&plain);
    // Truncated ZIP.
    let broken = write_temp("broken.xlsx", b"PK\x03\x04truncated");
    assert!(load_workbook(&broken).is_err());
    let _ = std::fs::remove_file(&broken);
    // ZIP without any spreadsheet part.
    let empty_zip = write_temp("emptyzip.xlsx", &zip_store(&[("readme.txt", b"hi")]));
    let err = load_workbook(&empty_zip).expect_err("no spreadsheet part must fail");
    assert!(err.contains("no spreadsheet"), "got {err:?}");
    let _ = std::fs::remove_file(&empty_zip);
    // Word archive reports its kind honestly.
    let doc = write_temp(
      "doc.xlsx",
      &zip_store(&[("word/document.xml", b"<document/>")]),
    );
    let err = load_workbook(&doc).expect_err("word archive must fail");
    assert!(err.contains("word document"), "got {err:?}");
    let _ = std::fs::remove_file(&doc);
    // Legacy OLE reports the legacy hint.
    let ole = write_temp("legacy.xls", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00]);
    let err = load_workbook(&ole).expect_err("legacy xls must fail");
    assert!(err.contains("legacy .xls"), "got {err:?}");
    let _ = std::fs::remove_file(&ole);
    // Password-protected xlsx reports the password hint.
    let ole_xlsx = write_temp("locked.xlsx", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00]);
    let err = load_workbook(&ole_xlsx).expect_err("locked xlsx must fail");
    assert!(err.contains("password"), "got {err:?}");
    let _ = std::fs::remove_file(&ole_xlsx);
    // ODT content reports its kind honestly.
    let odt = write_temp(
      "text.xlsx",
      &zip_store(&[("content.xml", b"<office:document><office:text/></office:document>")]),
    );
    let err = load_workbook(&odt).expect_err("odt content must fail");
    assert!(err.contains("word document"), "got {err:?}");
    let _ = std::fs::remove_file(&odt);
    // Binary CSV is rejected, never parsed as text.
    let bin_csv = write_temp("binary.csv", b"a,b\x00c\n");
    assert!(load_workbook(&bin_csv).is_err());
    let _ = std::fs::remove_file(&bin_csv);
  }

  #[test]
  fn grid_helpers() {
    assert_eq!(col_letters(0), "A");
    assert_eq!(col_letters(25), "Z");
    assert_eq!(col_letters(26), "AA");
    assert_eq!(col_letters(27), "AB");
    assert_eq!(col_letters(51), "AZ");
    assert_eq!(col_letters(52), "BA");
    assert_eq!(parse_cell_ref("A1"), Some((0, 0)));
    assert_eq!(parse_cell_ref("B2"), Some((1, 1)));
    assert_eq!(parse_cell_ref("Z10"), Some((9, 25)));
    assert_eq!(parse_cell_ref("AA1"), Some((0, 26)));
    assert_eq!(parse_cell_ref("$B$2"), Some((1, 1)));
    assert_eq!(parse_cell_ref("a1"), Some((0, 0)));
    assert_eq!(parse_cell_ref("A0"), None);
    assert_eq!(parse_cell_ref("1A"), None);
    assert_eq!(parse_cell_ref(""), None);
    assert!(is_number_text("42"));
    assert!(is_number_text("-3.5"));
    assert!(!is_number_text(""));
    assert!(!is_number_text("TRUE"));
    assert!(!is_number_text("12.5%"));
  }

  #[test]
  fn huge_sheets_truncate() {
    let mut csv = String::new();
    for i in 0..1200 {
      csv.push_str(&format!("r{i},v{i}\n"));
    }
    let path = write_temp("huge.csv", csv.as_bytes());
    let book = load_workbook(&path).expect("huge csv loads");
    assert!(book.truncated);
    assert_eq!(book.sheets[0].rows.len(), MAX_SHEET_ROWS);
    assert_eq!(book.sheet_dims(0), (MAX_SHEET_ROWS, 2));
    let _ = std::fs::remove_file(&path);
    // Wide rows truncate at MAX_SHEET_COLS.
    let wide = "h,".repeat(MAX_SHEET_COLS + 10) + "\n";
    let path = write_temp("wide.csv", wide.as_bytes());
    let book = load_workbook(&path).expect("wide csv loads");
    assert!(book.truncated);
    assert_eq!(book.sheet_dims(0).1, MAX_SHEET_COLS);
    let _ = std::fs::remove_file(&path);
  }
}
