//! Read-only word document support for Preview.
//!
//! `.docx` files are ZIP packages holding `word/document.xml` (Office Open
//! XML). `.odt` files are ZIP packages holding `content.xml` (OpenDocument).
//! `.rtf` files are plain-text markup. Legacy `.doc` (OLE compound) files
//! have no decoder here and report an honest hint. Parsing uses only crates
//! already in the `Cargo.lock` closure (`flate2` for deflated ZIP entries,
//! `quick-xml` for the document XML), so no new crate family is introduced.
//! Rendering targets a read-only `GtkTextView` with text tags (SF Pro
//! Display, larger bold headings), reusing the Markdown tag pattern from
//! `src/markdown.rs`. Documents are never editable and never saved.

use std::path::Path;

use gtk::prelude::*;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Max decompressed XML parsed per ZIP entry (same budget as text files).
const MAX_PART_BYTES: u64 = crate::model::MAX_TEXT_BYTES;

/// Max rendered blocks per document; longer documents stop with
/// `truncated` set so huge files stay cheap to display.
const MAX_BLOCKS: usize = 20_000;

/// One inline run of text with bold/italic flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
  /// Visible text of the run.
  pub text: String,
  /// Bold flag (`w:b`, `fo:font-weight="bold"`, `\b`).
  pub bold: bool,
  /// Italic flag (`w:i`, `fo:font-style="italic"`, `\i`).
  pub italic: bool,
}

/// One rendered block of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
  /// Heading with level 1..=3 plus inline runs.
  Heading {
    /// Heading level (`1`, `2` or `3`).
    level: u8,
    /// Inline runs of the heading.
    spans: Vec<Span>,
  },
  /// Normal paragraph plus inline runs.
  Paragraph(Vec<Span>),
  /// List item (bulleted and numbered lists both render as bullets).
  Bullet(Vec<Span>),
  /// Table as a plain grid: rows of plain-text cells.
  Table(Vec<Vec<String>>),
}

/// A loaded word document: formatted blocks plus file metadata.
#[derive(Clone, Debug)]
pub struct Document {
  /// Short format label (`DOCX`, `ODT`, `RTF`, `DOC`).
  pub format: String,
  /// Rendered blocks in document order.
  pub blocks: Vec<Block>,
  /// File size in bytes.
  pub file_bytes: u64,
  /// True when the block cap stopped the parse (rest is omitted).
  pub truncated: bool,
}

impl Document {
  /// Number of text blocks (headings, paragraphs, list items).
  pub fn paragraph_count(&self) -> usize {
    self
      .blocks
      .iter()
      .filter(|b| !matches!(b, Block::Table(_)))
      .count()
  }

  /// Number of whitespace-separated words over runs and table cells.
  pub fn word_count(&self) -> usize {
    self
      .blocks
      .iter()
      .map(|block| match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Bullet(spans) => {
          spans.iter().map(|s| s.text.split_whitespace().count()).sum::<usize>()
        }
        Block::Table(rows) => rows
          .iter()
          .map(|row| row.iter().map(|c| c.split_whitespace().count()).sum::<usize>())
          .sum::<usize>(),
      })
      .sum::<usize>()
  }
}

/// Load a word document for read-only preview. Returns a human-readable
/// reason for missing files, directories, empty files, oversized files,
/// password-protected packages, legacy `.doc` files and corrupt content.
/// Never panics on corrupt input.
pub fn load_document(path: &Path) -> Result<Document, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if meta.len() == 0 {
    return Err("file is empty".to_string());
  }
  if meta.len() > crate::model::MAX_TEXT_BYTES {
    return Err("file is too large for the document viewer (over 8 MiB)".to_string());
  }
  let bytes = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  let format = crate::model::document_format_label(&ext).to_string();
  if is_zip_magic(&bytes) {
    return load_zip_package(&bytes, format, meta.len());
  }
  if crate::model::is_rtf_markup(&bytes) {
    let text = decode_rtf_bytes(&bytes);
    let (blocks, truncated) = parse_rtf(&text);
    return Ok(Document { format, blocks, file_bytes: meta.len(), truncated });
  }
  if is_ole_magic(&bytes) {
    return Err(ole_hint(&ext));
  }
  Err("file is not a recognized document (not a Word, ODT, RTF or legacy Word file)".to_string())
}

/// Hint for OLE compound files: password-protected OOXML packages use OLE
/// too, so `.docx`/`.odt` extensions report the password hint while `.doc`
/// reports the legacy hint.
fn ole_hint(ext: &str) -> String {
  if ext == "docx" || ext == "odt" {
    "file is encrypted (password-protected); open it without a password is not supported"
      .to_string()
  } else {
    "legacy .doc format is not parsed (this includes password-protected files); convert it to .docx to preview it"
      .to_string()
  }
}

fn is_zip_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04])
}

fn is_ole_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
}

/// Load a ZIP-based package: route to the `.docx` or `.odt` parser by its
/// entries, or report an honest hint for other archives (spreadsheets,
/// presentations, plain zips).
fn load_zip_package(bytes: &[u8], format: String, file_bytes: u64) -> Result<Document, String> {
  let entries = central_dir(bytes)?;
  let has = |name: &str| entries.iter().any(|e| e.name == name);
  if has("word/document.xml") {
    let xml = extract(&entries, bytes, "word/document.xml")?;
    let (blocks, truncated) = parse_docx_xml(&xml);
    return Ok(Document { format, blocks, file_bytes, truncated });
  }
  if has("content.xml") {
    let xml = extract(&entries, bytes, "content.xml")?;
    let (blocks, truncated) = parse_odt_xml(&xml);
    return Ok(Document { format, blocks, file_bytes, truncated });
  }
  if has("xl/workbook.xml") {
    return Err("this is a spreadsheet, only word documents (.docx) are supported".to_string());
  }
  if has("ppt/presentation.xml") {
    return Err("this is a presentation, only word documents (.docx) are supported".to_string());
  }
  Err("ZIP archive holds no word document (missing word/document.xml)".to_string())
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
  let off = entry.local_offset as usize;
  if bytes.get(off..off + 4) != Some(&[0x50, 0x4B, 0x03, 0x04][..]) {
    return Err("ZIP entry header is corrupt".to_string());
  }
  let name_len = read_u16(bytes, off + 26).ok_or_else(|| "ZIP entry is corrupt".to_string())? as usize;
  let extra_len = read_u16(bytes, off + 28).ok_or_else(|| "ZIP entry is corrupt".to_string())? as usize;
  let data_off = off.saturating_add(30).saturating_add(name_len).saturating_add(extra_len);
  let comp_size = entry.comp_size.min(bytes.len() as u64) as usize;
  let comp = bytes.get(data_off..data_off.saturating_add(comp_size)).ok_or_else(|| {
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
        return Err("document part is too large for the viewer (over 8 MiB)".to_string());
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
    .map_err(|e| format!("cannot decompress document part: {e}"))?;
  if out.len() as u64 > MAX_PART_BYTES {
    return Err("document part is too large for the viewer (over 8 MiB)".to_string());
  }
  Ok(out)
}

/// Strip a namespace prefix (`w:p` becomes `p`).
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

/// Push a finished paragraph block unless it holds no text at all.
fn push_text_block(blocks: &mut Vec<Block>, kind: ParaKind, spans: Vec<Span>) -> bool {
  if !spans.iter().any(|s| !s.text.is_empty()) {
    return false;
  }
  let block = match kind {
    ParaKind::Heading(level) => Block::Heading { level, spans },
    ParaKind::Bullet => Block::Bullet(spans),
    ParaKind::Plain => Block::Paragraph(spans),
  };
  blocks.push(block);
  true
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParaKind {
  Plain,
  Bullet,
  Heading(u8),
}

/// Map a `w:pStyle` value to a heading level (`Title` and `Heading1` map to
/// 1, `Heading2` to 2, deeper headings to 3). Returns `None` for body text.
fn docx_heading(style: &str) -> Option<u8> {
  let lower = style.to_lowercase();
  if lower == "title" {
    return Some(1);
  }
  let rest = lower.strip_prefix("heading")?;
  match rest.parse::<u8>() {
    Ok(1) => Some(1),
    Ok(2) => Some(2),
    Ok(_) => Some(3),
    Err(_) => None,
  }
}

/// Collector for one `.docx` paragraph: style name, list flag and runs.
#[derive(Default)]
struct DocxPara {
  style: Option<String>,
  is_list: bool,
  spans: Vec<Span>,
  text: String,
  bold: bool,
  italic: bool,
}

impl DocxPara {
  fn flush_run(&mut self) {
    if self.text.is_empty() {
      return;
    }
    self.spans.push(Span { text: std::mem::take(&mut self.text), bold: self.bold, italic: self.italic });
  }
}

/// Parse `word/document.xml` into blocks: paragraphs, headings (`pStyle`
/// `Title`/`Heading1..3`, deeper levels map to 3), bold/italic runs,
/// bulleted and numbered lists (both render as bullets) and tables as a
/// plain grid. Deleted text (`w:del`) and field codes (`w:instrText`) are
/// skipped. Returns the blocks plus the truncation flag.
fn parse_docx_xml(xml: &[u8]) -> (Vec<Block>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut blocks: Vec<Block> = Vec::new();
  let mut truncated = false;
  let mut para: Option<DocxPara> = None;
  // True inside `w:t`: only there is character data body text (inter-tag
  // whitespace is ignored, so no reader trimming is needed and
  // `xml:space="preserve"` spacing survives).
  let mut in_text_elem = false;
  let mut in_run_props = false;
  let mut run_bold = false;
  let mut run_italic = false;
  let mut in_para_props = false;
  let mut skip_depth: usize = 0;
  let mut depth: usize = 0;
  // Table state: only the outermost table builds a grid; nested tables
  // merge their text into the current cell.
  let mut table_depth: usize = 0;
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut cell_paras: Vec<String> = Vec::new();

  macro_rules! capped {
    () => {
      if blocks.len() >= MAX_BLOCKS {
        truncated = true;
        return (blocks, truncated);
      }
    };
  }

  let finish_para = |para: &mut DocxPara, blocks: &mut Vec<Block>, cell_paras: &mut Vec<String>, table_depth: usize| {
    para.flush_run();
    if table_depth > 0 {
      let text: String = para
        .spans
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("");
      if !text.trim().is_empty() {
        cell_paras.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
      }
    } else {
      let kind = if let Some(style) = para.style.as_deref() {
        if let Some(level) = docx_heading(style) {
          ParaKind::Heading(level)
        } else if para.is_list {
          ParaKind::Bullet
        } else {
          ParaKind::Plain
        }
      } else if para.is_list {
        ParaKind::Bullet
      } else {
        ParaKind::Plain
      };
      push_text_block(blocks, kind, std::mem::take(&mut para.spans));
    }
    para.style = None;
    para.is_list = false;
    para.bold = false;
    para.italic = false;
  };

  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        depth += 1;
        let name = local(e.name().into_inner()).to_vec();
        if skip_depth > 0 {
          continue;
        }
        match name.as_slice() {
          b"del" | b"moveFrom" => skip_depth = depth,
          b"p" => {
            para = Some(DocxPara::default());
          }
          b"pPr" => in_para_props = true,
          b"pStyle" => {
            if in_para_props {
              if let Some(val) = attr_value(e, b"val") {
                if let Some(p) = para.as_mut() {
                  p.style = Some(val);
                }
              }
            }
          }
          b"numPr" => {
            if let Some(p) = para.as_mut() {
              p.is_list = true;
            }
          }
          b"r" => {
            // A new run starts with default formatting; its `rPr` sets the
            // flags below. Flush first so the previous text keeps its flags.
            if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = false;
              p.italic = false;
            }
            run_bold = false;
            run_italic = false;
          }
          b"rPr" => in_run_props = true,
          b"b" | b"bCs" => {
            if in_run_props {
              let on = attr_value(e, b"val").is_none_or(|v| {
                let lower = v.to_lowercase();
                lower != "false" && lower != "0" && lower != "off"
              });
              run_bold = on;
            }
          }
          b"i" | b"iCs" => {
            if in_run_props {
              let on = attr_value(e, b"val").is_none_or(|v| {
                let lower = v.to_lowercase();
                lower != "false" && lower != "0" && lower != "off"
              });
              run_italic = on;
            }
          }
          b"tbl" => {
            if table_depth == 0 {
              rows.clear();
            }
            table_depth += 1;
          }
          b"t" => in_text_elem = true,
          b"tr" => {
            if table_depth == 1 {
              row.clear();
            }
          }
          b"tc" => {
            if table_depth == 1 {
              cell_paras.clear();
            }
          }
          _ => {}
        }
      }
      Ok(Event::Empty(ref e)) => {
        let name = local(e.name().into_inner()).to_vec();
        if skip_depth > 0 {
          continue;
        }
        match name.as_slice() {
          b"b" | b"bCs" => {
            if in_run_props {
              run_bold = true;
            } else if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = true;
            }
          }
          b"i" | b"iCs" => {
            if in_run_props {
              run_italic = true;
            } else if let Some(p) = para.as_mut() {
              p.flush_run();
              p.italic = true;
            }
          }
          b"pStyle" => {
            if in_para_props {
              if let Some(val) = attr_value(e, b"val") {
                if let Some(p) = para.as_mut() {
                  p.style = Some(val);
                }
              }
            }
          }
          b"numPr" => {
            if let Some(p) = para.as_mut() {
              p.is_list = true;
            }
          }
          b"tab" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\t');
            }
          }
          b"br" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\n');
            }
          }
          b"noBreakHyphen" => {
            if let Some(p) = para.as_mut() {
              p.text.push('-');
            }
          }
          _ => {}
        }
      }
      Ok(Event::End(ref e)) => {
        let name = local(e.name().into_inner()).to_vec();
        if skip_depth > 0 {
          if depth == skip_depth {
            skip_depth = 0;
          }
          depth = depth.saturating_sub(1);
          continue;
        }
        match name.as_slice() {
          b"p" => {
            if let Some(mut p) = para.take() {
              capped!();
              finish_para(&mut p, &mut blocks, &mut cell_paras, table_depth);
            }
          }
          b"t" => in_text_elem = false,
          b"pPr" => in_para_props = false,
          b"rPr" => {
            in_run_props = false;
            if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = run_bold;
              p.italic = run_italic;
            }
          }
          b"tc" => {
            if table_depth == 1 {
              let text = cell_paras.join(" ");
              row.push(text);
            }
          }
          b"tr" => {
            if table_depth == 1 && !row.is_empty() {
              rows.push(std::mem::take(&mut row));
            }
          }
          b"tbl" => {
            table_depth = table_depth.saturating_sub(1);
            if table_depth == 0 && !rows.is_empty() {
              capped!();
              blocks.push(Block::Table(std::mem::take(&mut rows)));
            }
          }
          _ => {}
        }
        depth = depth.saturating_sub(1);
      }
      Ok(Event::Text(ref e)) => {
        if skip_depth > 0 || !in_text_elem {
          continue;
        }
        if let Some(p) = para.as_mut() {
          p.text.push_str(&text_content(e));
        }
      }
      Ok(Event::CData(ref e)) => {
        if skip_depth > 0 || !in_text_elem {
          continue;
        }
        if let Some(p) = para.as_mut() {
          p.text.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }
  if let Some(mut p) = para.take() {
    finish_para(&mut p, &mut blocks, &mut cell_paras, table_depth);
  }
  if table_depth > 0 && !rows.is_empty() && blocks.len() < MAX_BLOCKS {
    blocks.push(Block::Table(std::mem::take(&mut rows)));
  }
  (blocks, truncated)
}

/// Parse `content.xml` of an ODT package: headings (`text:h` with
/// `text:outline-level`), paragraphs, bold/italic spans via automatic
/// styles, lists as bullets and tables as a plain grid. List numbering is
/// not resolved, so numbered lists render as bullets.
fn parse_odt_xml(xml: &[u8]) -> (Vec<Block>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut blocks: Vec<Block> = Vec::new();
  let mut truncated = false;
  // Automatic style name mapped to (bold, italic).
  let mut styles: std::collections::HashMap<String, (bool, bool)> = std::collections::HashMap::new();
  let mut current_style: Option<String> = None;
  let mut style_bold = false;
  let mut style_italic = false;
  let mut para: Option<DocxPara> = None;
  let mut para_level: Option<u8> = None;
  let mut list_depth: usize = 0;
  // Span stack for nested `text:span` elements (outer, ...).
  let mut span_stack: Vec<(bool, bool)> = Vec::new();
  let mut table_depth: usize = 0;
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut cell_paras: Vec<String> = Vec::new();

  macro_rules! capped {
    () => {
      if blocks.len() >= MAX_BLOCKS {
        truncated = true;
        return (blocks, truncated);
      }
    };
  }

  let finish_para = |para: &mut DocxPara,
                     level: Option<u8>,
                     in_list: bool,
                     blocks: &mut Vec<Block>,
                     cell_paras: &mut Vec<String>,
                     table_depth: usize| {
    para.flush_run();
    if table_depth > 0 {
      let text: String = para.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
      if !text.trim().is_empty() {
        cell_paras.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
      }
    } else {
      let kind = if let Some(level) = level {
        ParaKind::Heading(level.clamp(1, 3))
      } else if in_list {
        ParaKind::Bullet
      } else {
        ParaKind::Plain
      };
      push_text_block(blocks, kind, std::mem::take(&mut para.spans));
    }
    para.bold = false;
    para.italic = false;
  };

  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        let name = local(e.name().into_inner()).to_vec();
        match name.as_slice() {
          b"style" => {
            current_style = attr_value(e, b"name");
            style_bold = false;
            style_italic = false;
          }
          b"text-properties" => {
            if let Some(weight) = attr_value(e, b"font-weight") {
              if weight.to_lowercase() == "bold" {
                style_bold = true;
              }
            }
            if let Some(style) = attr_value(e, b"font-style") {
              if style.to_lowercase() == "italic" {
                style_italic = true;
              }
            }
          }
          b"p" => {
            let mut p = DocxPara::default();
            if let Some(style) = attr_value(e, b"style-name") {
              if let Some((bold, italic)) = styles.get(&style) {
                p.bold = *bold;
                p.italic = *italic;
              }
            }
            para = Some(p);
            para_level = None;
          }
          b"h" => {
            let mut p = DocxPara::default();
            if let Some(style) = attr_value(e, b"style-name") {
              if let Some((bold, italic)) = styles.get(&style) {
                p.bold = *bold;
                p.italic = *italic;
              }
            }
            let level = attr_value(e, b"outline-level")
              .and_then(|v| v.parse::<u8>().ok())
              .unwrap_or(1)
              .clamp(1, 3);
            para = Some(p);
            para_level = Some(level);
          }
          b"span" => {
            if let Some(p) = para.as_mut() {
              p.flush_run();
              span_stack.push((p.bold, p.italic));
              if let Some(style) = attr_value(e, b"style-name") {
                if let Some((bold, italic)) = styles.get(&style) {
                  p.bold = *bold;
                  p.italic = *italic;
                }
              }
            }
          }
          b"list" => list_depth += 1,
          b"table" => {
            if table_depth == 0 {
              rows.clear();
            }
            table_depth += 1;
          }
          b"table-row" => {
            if table_depth == 1 {
              row.clear();
            }
          }
          b"table-cell" => {
            if table_depth == 1 {
              cell_paras.clear();
            }
          }
          _ => {}
        }
      }
      Ok(Event::Empty(ref e)) => {
        let name = local(e.name().into_inner()).to_vec();
        match name.as_slice() {
          b"line-break" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\n');
            }
          }
          b"tab" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\t');
            }
          }
          b"s" => {
            let count = attr_value(e, b"c").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            if let Some(p) = para.as_mut() {
              for _ in 0..count.min(64) {
                p.text.push(' ');
              }
            }
          }
          // Self-closing style definitions (`<style:text-properties .../>`).
          b"text-properties" => {
            if let Some(weight) = attr_value(e, b"font-weight") {
              if weight.to_lowercase() == "bold" {
                style_bold = true;
              }
            }
            if let Some(style) = attr_value(e, b"font-style") {
              if style.to_lowercase() == "italic" {
                style_italic = true;
              }
            }
          }
          _ => {}
        }
      }
      Ok(Event::End(ref e)) => {
        let name = local(e.name().into_inner()).to_vec();
        match name.as_slice() {
          b"style" => {
            if let Some(style) = current_style.take() {
              styles.insert(style, (style_bold, style_italic));
            }
          }
          b"p" | b"h" => {
            if let Some(mut p) = para.take() {
              capped!();
              let level = para_level.take();
              let in_list = list_depth > 0;
              finish_para(&mut p, level, in_list, &mut blocks, &mut cell_paras, table_depth);
            }
          }
          b"span" => {
            if let Some(p) = para.as_mut() {
              p.flush_run();
              if let Some((bold, italic)) = span_stack.pop() {
                p.bold = bold;
                p.italic = italic;
              }
            }
          }
          b"list" => list_depth = list_depth.saturating_sub(1),
          b"table-cell" => {
            if table_depth == 1 {
              row.push(cell_paras.join(" "));
            }
          }
          b"table-row" => {
            if table_depth == 1 && !row.is_empty() {
              rows.push(std::mem::take(&mut row));
            }
          }
          b"table" => {
            table_depth = table_depth.saturating_sub(1);
            if table_depth == 0 && !rows.is_empty() {
              capped!();
              blocks.push(Block::Table(std::mem::take(&mut rows)));
            }
          }
          _ => {}
        }
      }
      Ok(Event::Text(ref e)) => {
        // Inter-tag pretty-print whitespace is junk (real line breaks use
        // `text:line-break`); all other spacing is kept verbatim so words
        // around spans do not glue together.
        let text = text_content(e);
        if text.trim().is_empty() && (text.contains('\n') || text.contains('\r')) {
          continue;
        }
        if let Some(p) = para.as_mut() {
          p.text.push_str(&text);
        }
      }
      Ok(Event::CData(ref e)) => {
        if let Some(p) = para.as_mut() {
          p.text.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }
  (blocks, truncated)
}

/// Decode RTF bytes as Windows-1252 (the usual `\ansicpg1252` default) with
/// a lossy fallback so no byte sequence can fail the load.
fn decode_rtf_bytes(bytes: &[u8]) -> String {
  bytes.iter().map(|&b| win1252_char(b)).collect()
}

/// Map one byte to a char using Windows-1252 for 0x80..=0x9F and Latin-1
/// elsewhere (ASCII is identical in both).
fn win1252_char(byte: u8) -> char {
  const TABLE: [char; 32] = [
    '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}',
    '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
  ];
  if (0x80..=0x9F).contains(&byte) {
    TABLE[(byte - 0x80) as usize]
  } else {
    byte as char
  }
}

/// Parse RTF markup into blocks on a best-effort basis: `\par` ends a
/// paragraph, `\b`/`\i` toggle bold/italic, `\cell` separates table cells
/// with ` | `, `\row` ends a table row, `\'hh` and `\uN` decode escapes.
/// Metadata destinations (`fonttbl`, `colortbl`, `stylesheet`, `info`,
/// `pict`, `object`, `field`) are skipped. Unknown controls are ignored,
/// so exotic files degrade to plain text instead of failing.
fn parse_rtf(text: &str) -> (Vec<Block>, bool) {
  let chars: Vec<char> = text.chars().collect();
  let mut blocks: Vec<Block> = Vec::new();
  let mut truncated = false;
  let mut spans: Vec<Span> = Vec::new();
  let mut current = String::new();
  let mut bold = false;
  let mut italic = false;
  // Formatting stack for `{` groups plus a skip depth for destinations.
  let mut fmt_stack: Vec<(bool, bool)> = Vec::new();
  let mut depth: usize = 0;
  let mut skip_below: Option<usize> = None;
  let mut pos = 0;

  macro_rules! flush_run {
    () => {
      if !current.is_empty() {
        spans.push(Span { text: std::mem::take(&mut current), bold, italic });
      }
    };
  }
  macro_rules! flush_para {
    () => {
      flush_run!();
      if spans.iter().any(|s| !s.text.is_empty()) {
        if blocks.len() >= MAX_BLOCKS {
          truncated = true;
          return (blocks, truncated);
        }
        blocks.push(Block::Paragraph(std::mem::take(&mut spans)));
      } else {
        spans.clear();
      }
    };
  }
  // Destination groups whose content is metadata, never body text.
  const SKIP: [&str; 7] = ["fonttbl", "colortbl", "stylesheet", "info", "pict", "object", "field"];

  let skipped = |skip_below: Option<usize>, depth: usize| skip_below.is_some_and(|s| depth >= s);

  while pos < chars.len() {
    let c = chars[pos];
    if c == '{' {
      if !skipped(skip_below, depth) {
        flush_run!();
      }
      fmt_stack.push((bold, italic));
      depth += 1;
      pos += 1;
      continue;
    }
    if c == '}' {
      if !skipped(skip_below, depth) {
        flush_run!();
      }
      if let Some((b, i)) = fmt_stack.pop() {
        bold = b;
        italic = i;
      }
      depth = depth.saturating_sub(1);
      if skip_below.is_some_and(|s| depth < s) {
        skip_below = None;
      }
      pos += 1;
      continue;
    }
    if c == '\\' {
      pos += 1;
      if pos >= chars.len() {
        break;
      }
      let n = chars[pos];
      // Literal escaped chars `\{`, `\}`, `\\`.
      if n == '{' || n == '}' || n == '\\' {
        if !skipped(skip_below, depth) {
          current.push(n);
        }
        pos += 1;
        continue;
      }
      // Single-char controls: `\~` nbsp, `\-` hyphen, `\_` hyphen.
      if n == '~' {
        if !skipped(skip_below, depth) {
          current.push('\u{00A0}');
        }
        pos += 1;
        continue;
      }
      if n == '-' || n == '_' {
        if !skipped(skip_below, depth) {
          current.push('-');
        }
        pos += 1;
        continue;
      }
      // `\*` destination marker: the group content decides, keep parsing.
      if n == '*' {
        pos += 1;
        continue;
      }
      // Hex escape `\'hh`.
      if n == '\'' {
        if pos + 2 < chars.len() {
          let hex: String = [chars[pos + 1], chars[pos + 2]].iter().collect();
          if let Ok(byte) = u8::from_str_radix(&hex, 16) {
            if !skipped(skip_below, depth) {
              current.push(win1252_char(byte));
            }
          }
          pos += 3;
        } else {
          pos += 1;
        }
        continue;
      }
      // Control word: letters plus an optional signed numeric parameter.
      if n.is_ascii_alphabetic() {
        let start = pos;
        while pos < chars.len() && chars[pos].is_ascii_alphabetic() {
          pos += 1;
        }
        let word: String = chars[start..pos].iter().collect();
        let mut param: Option<i32> = None;
        if pos < chars.len() && (chars[pos] == '-' || chars[pos].is_ascii_digit()) {
          let num_start = pos;
          if chars[pos] == '-' {
            pos += 1;
          }
          while pos < chars.len() && chars[pos].is_ascii_digit() {
            pos += 1;
          }
          param = chars[num_start..pos].iter().collect::<String>().parse::<i32>().ok();
        }
        // A single space after a control word is its delimiter.
        if pos < chars.len() && chars[pos] == ' ' {
          pos += 1;
        }
        if SKIP.contains(&word.as_str()) {
          skip_below = Some(depth);
          continue;
        }
        if skipped(skip_below, depth) {
          continue;
        }
        match word.as_str() {
          "b" => {
            flush_run!();
            bold = param.unwrap_or(1) != 0;
          }
          "i" => {
            flush_run!();
            italic = param.unwrap_or(1) != 0;
          }
          "par" | "line" | "row" => {
            flush_para!();
          }
          "tab" | "cell" => {
            if word == "cell" {
              current.push_str(" | ");
            } else {
              current.push('\t');
            }
          }
          "u" => {
            if let Some(n) = param {
              let code = if n < 0 { n as i64 + 65536 } else { n as i64 } as u32;
              if let Some(ch) = char::from_u32(code) {
                current.push(ch);
              }
              // Skip the single-byte fallback char (unless it opens a new
              // construct, in which case it belongs to the parser).
              if pos < chars.len() && !matches!(chars[pos], '{' | '}' | '\\') {
                pos += 1;
              }
            }
          }
          _ => {}
        }
        continue;
      }
      // Anything else after a backslash is ignored (e.g. a bare newline).
      pos += 1;
      continue;
    }
    if c == '\n' || c == '\r' {
      // Bare line breaks in the source are formatting, not paragraphs.
      pos += 1;
      continue;
    }
    if !skipped(skip_below, depth) {
      current.push(c);
    }
    pos += 1;
  }
  flush_run!();
  if spans.iter().any(|s| !s.text.is_empty()) {
    blocks.push(Block::Paragraph(spans));
  }
  (blocks, truncated)
}

/// Render a loaded document into a read-only `GtkTextView` buffer with text
/// tags (SF Pro Display, larger bold headings), following the Markdown tag
/// pattern from `src/markdown.rs`. Tables render as plain `a | b` grid
/// rows. Truncated documents end with a dim `docx.truncated` hint line.
pub fn render_into(buffer: &gtk::TextBuffer, doc: &Document) {
  ensure_tags(buffer);
  buffer.set_text("");
  let mut iter = buffer.start_iter();
  for block in &doc.blocks {
    match block {
      Block::Heading { level, spans } => {
        let tag = match level {
          1 => "h1",
          2 => "h2",
          _ => "h3",
        };
        insert_spans(&mut iter, spans, &[tag]);
        insert_with(&mut iter, "\n", &[]);
      }
      Block::Paragraph(spans) => {
        insert_spans(&mut iter, spans, &[]);
        insert_with(&mut iter, "\n", &[]);
      }
      Block::Bullet(spans) => {
        insert_with(&mut iter, "• ", &["dim"]);
        insert_spans(&mut iter, spans, &[]);
        insert_with(&mut iter, "\n", &[]);
      }
      Block::Table(rows) => {
        for row in rows {
          let mut first = true;
          for cell in row {
            if !first {
              insert_with(&mut iter, " | ", &["dim"]);
            }
            first = false;
            insert_with(&mut iter, cell, &[]);
          }
          insert_with(&mut iter, "\n", &[]);
        }
      }
    }
  }
  if doc.truncated {
    insert_with(
      &mut iter,
      &crate::lang::t_with("docx.truncated", &[("count", &doc.blocks.len().to_string())]),
      &["dim"],
    );
    insert_with(&mut iter, "\n", &[]);
  }
}

fn ensure_tags(buffer: &gtk::TextBuffer) {
  let table = buffer.tag_table();
  let ensure = |name: &str, props: &[(&str, glib::Value)]| {
    if table.lookup(name).is_none() {
      let tag = gtk::TextTag::new(Some(name));
      for (prop, value) in props {
        tag.set_property(*prop, value);
      }
      table.add(&tag);
    }
  };
  ensure("h1", &[("weight", 700.into()), ("scale", 1.6.into())]);
  ensure("h2", &[("weight", 700.into()), ("scale", 1.35.into())]);
  ensure("h3", &[("weight", 700.into()), ("scale", 1.15.into())]);
  ensure("bold", &[("weight", 700.into())]);
  ensure("italic", &[("style", 2.into())]);
  ensure("dim", &[("foreground", "#888888".into())]);
}

fn insert_with(iter: &mut gtk::TextIter, text: &str, tags: &[&str]) {
  let buffer = iter.buffer();
  if tags.is_empty() {
    buffer.insert(iter, text);
  } else {
    let owned: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
    let refs: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    buffer.insert_with_tags_by_name(iter, text, &refs);
  }
}

fn insert_spans(iter: &mut gtk::TextIter, spans: &[Span], base: &[&str]) {
  for span in spans {
    let mut tags: Vec<&str> = base.to_vec();
    if span.bold {
      tags.push("bold");
    }
    if span.italic {
      tags.push("italic");
    }
    insert_with(iter, &span.text, &tags);
  }
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

  /// Same writer with one deflated entry (method 8) for the inflate path.
  fn zip_deflated(name: &str, data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).expect("deflate test data");
    let comp = encoder.finish().expect("finish deflate");
    let mut buf = Vec::new();
    buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
    buf.extend_from_slice(&[0x14, 0x00, 0x00, 0x00, 0x08, 0x00]);
    buf.extend_from_slice(&[0x00, 0x00, 0x21, 0x00]);
    buf.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
    buf.extend_from_slice(&[0x00, 0x00]);
    buf.extend_from_slice(name.as_bytes());
    buf.extend_from_slice(&comp);
    let cd_offset = buf.len();
    let mut central = Vec::new();
    central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
    central.extend_from_slice(&[0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00]);
    central.extend_from_slice(&[0x00, 0x00, 0x21, 0x00]);
    central.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    central.extend_from_slice(&(comp.len() as u32).to_le_bytes());
    central.extend_from_slice(&(data.len() as u32).to_le_bytes());
    central.extend_from_slice(&(name.len() as u16).to_le_bytes());
    central.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(name.as_bytes());
    buf.extend_from_slice(&central);
    let cd_size = buf.len() - cd_offset;
    buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00]);
    buf.extend_from_slice(&(cd_size as u32).to_le_bytes());
    buf.extend_from_slice(&(cd_offset as u32).to_le_bytes());
    buf.extend_from_slice(&[0x00, 0x00]);
    buf
  }

  const SAMPLE_DOCX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Hello Title</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>Italic</w:t></w:r><w:r><w:t> plain &amp; done</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>First item</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
</w:body></w:document>"#;

  const SAMPLE_ODT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0">
<office:automatic-styles><style:style style:name="T1"><style:text-properties fo:font-weight="bold" fo:font-style="italic"/></style:style></office:automatic-styles>
<office:body><office:text>
<text:h text:outline-level="2">Section</text:h>
<text:p>Plain <text:span text:style-name="T1">styled</text:span> tail</text:p>
<text:list><text:list-item><text:p>Point</text:p></text:list-item></text:list>
<text:p></text:p>
</office:text></office:body></office:document-content>"#;

  fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("preview-docx-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write test document");
    path
  }

  #[test]
  fn docx_sample_parses() {
    let zip = zip_store(&[
      ("[Content_Types].xml", b"<Types/>"),
      ("word/document.xml", SAMPLE_DOCX.as_bytes()),
    ]);
    let path = write_temp("sample.docx", &zip);
    let doc = load_document(&path).expect("sample docx loads");
    assert_eq!(doc.format, "DOCX");
    assert!(!doc.truncated);
    assert_eq!(doc.blocks.len(), 4);
    match &doc.blocks[0] {
      Block::Heading { level: 1, spans } => assert_eq!(spans[0].text, "Hello Title"),
      other => panic!("expected h1, got {other:?}"),
    }
    match &doc.blocks[1] {
      Block::Paragraph(spans) => {
        assert_eq!(spans.len(), 3);
        assert!(spans[0].bold && !spans[0].italic);
        assert!(!spans[1].bold && spans[1].italic);
        assert_eq!(spans[2].text, " plain & done");
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    match &doc.blocks[2] {
      Block::Bullet(spans) => assert_eq!(spans[0].text, "First item"),
      other => panic!("expected bullet, got {other:?}"),
    }
    match &doc.blocks[3] {
      Block::Table(rows) => assert_eq!(rows, &vec![vec!["A1".to_string(), "B1".to_string()]]),
      other => panic!("expected table, got {other:?}"),
    }
    assert!(doc.word_count() > 0);
    assert_eq!(doc.paragraph_count(), 3);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn docx_deflated_entry_parses() {
    let zip = zip_deflated("word/document.xml", SAMPLE_DOCX.as_bytes());
    let path = write_temp("deflated.docx", &zip);
    let doc = load_document(&path).expect("deflated docx loads");
    assert_eq!(doc.blocks.len(), 4);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn odt_sample_parses() {
    let zip = zip_store(&[
      ("mimetype", b"application/vnd.oasis.opendocument.text"),
      ("content.xml", SAMPLE_ODT.as_bytes()),
    ]);
    let path = write_temp("sample.odt", &zip);
    let doc = load_document(&path).expect("sample odt loads");
    assert_eq!(doc.format, "ODT");
    assert_eq!(doc.blocks.len(), 3);
    match &doc.blocks[0] {
      Block::Heading { level: 2, spans } => assert_eq!(spans[0].text, "Section"),
      other => panic!("expected h2, got {other:?}"),
    }
    match &doc.blocks[1] {
      Block::Paragraph(spans) => {
        assert_eq!(spans.len(), 3);
        assert!(spans[1].bold && spans[1].italic);
        assert_eq!(spans[1].text, "styled");
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    match &doc.blocks[2] {
      Block::Bullet(spans) => assert_eq!(spans[0].text, "Point"),
      other => panic!("expected bullet, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn rtf_sample_parses() {
    let rtf = b"{\\rtf1\\ansi{\\fonttbl{\\f0 Arial;}}Hello {\\b bold\\b0} and {\\i italic\\i0}\\par Second \\'e9\\u233? line}";
    let path = write_temp("sample.rtf", rtf);
    let doc = load_document(&path).expect("sample rtf loads");
    assert_eq!(doc.format, "RTF");
    assert_eq!(doc.blocks.len(), 2);
    match &doc.blocks[0] {
      Block::Paragraph(spans) => {
        assert!(spans.iter().any(|s| s.bold && s.text.contains("bold")));
        assert!(spans.iter().any(|s| s.italic && s.text.contains("italic")));
        // The font table name must not leak into the text.
        assert!(!spans.iter().any(|s| s.text.contains("Arial")));
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    match &doc.blocks[1] {
      Block::Paragraph(spans) => {
        let text: String = spans.iter().map(|s| s.text.clone()).collect();
        assert!(text.contains('\u{e9}'), "hex escape decodes, got {text:?}");
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn corrupt_documents_hint_without_crash() {
    // Not a ZIP, RTF or OLE file.
    let plain = write_temp("bad.docx", b"just some text");
    assert!(load_document(&plain).is_err());
    let _ = std::fs::remove_file(&plain);
    // Truncated ZIP.
    let broken = write_temp("broken.docx", b"PK\x03\x04truncated");
    assert!(load_document(&broken).is_err());
    let _ = std::fs::remove_file(&broken);
    // ZIP without any document part.
    let empty_zip = write_temp("emptyzip.docx", &zip_store(&[("readme.txt", b"hi")]));
    let err = load_document(&empty_zip).expect_err("no document part must fail");
    assert!(err.contains("no word document"), "got {err:?}");
    let _ = std::fs::remove_file(&empty_zip);
    // Spreadsheet archive reports its kind honestly.
    let sheet = write_temp(
      "sheet.docx",
      &zip_store(&[("xl/workbook.xml", b"<workbook/>")]),
    );
    let err = load_document(&sheet).expect_err("spreadsheet must fail");
    assert!(err.contains("spreadsheet"), "got {err:?}");
    let _ = std::fs::remove_file(&sheet);
    // Legacy OLE reports the legacy hint.
    let ole = write_temp("legacy.doc", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00]);
    let err = load_document(&ole).expect_err("legacy doc must fail");
    assert!(err.contains("legacy .doc"), "got {err:?}");
    let _ = std::fs::remove_file(&ole);
    // Missing file reports stat failure.
    let missing = std::env::temp_dir().join("preview-docx-test/no-such-file.docx");
    assert!(load_document(&missing).is_err());
  }

  #[test]
  fn encrypted_entry_hints_password() {
    // Stored entry with the encrypted flag set (bit 0).
    let mut zip = zip_store(&[("word/document.xml", SAMPLE_DOCX.as_bytes())]);
    // Flip the general-purpose flag in the local header (offset 8).
    if zip.len() > 9 {
      zip[8] |= 0x01;
    }
    // Central directory flag lives after the local file; patch it too by
    // searching for the central signature.
    if let Some(pos) = zip.windows(4).position(|w| w == [0x50, 0x4B, 0x01, 0x02]) {
      zip[pos + 8] |= 0x01;
    }
    let path = write_temp("locked.docx", &zip);
    let err = load_document(&path).expect_err("encrypted entry must fail");
    assert!(err.contains("password"), "got {err:?}");
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn docx_heading_levels() {
    assert_eq!(docx_heading("Title"), Some(1));
    assert_eq!(docx_heading("Heading1"), Some(1));
    assert_eq!(docx_heading("Heading2"), Some(2));
    assert_eq!(docx_heading("Heading9"), Some(3));
    assert_eq!(docx_heading("Normal"), None);
  }

  #[test]
  fn rtf_escapes_decode() {
    assert_eq!(win1252_char(0xE9), '\u{e9}');
    assert_eq!(win1252_char(0x80), '\u{20AC}');
    assert_eq!(win1252_char(b'A'), 'A');
  }
}
