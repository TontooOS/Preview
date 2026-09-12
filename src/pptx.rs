//! Read-only presentation support for Preview.
//!
//! `.pptx` files are ZIP packages holding `ppt/presentation.xml` (slide
//! order), `ppt/_rels/presentation.xml.rels` (slide targets) and
//! `ppt/slides/slideN.xml` (Office Open XML DrawingML). `.odp` files are
//! ZIP packages holding `content.xml` (OpenDocument with `draw:page`
//! elements). Legacy `.ppt` (OLE compound) files have no decoder here and
//! report an honest hint. Parsing uses only crates already in the
//! `Cargo.lock` closure (`flate2` for deflated ZIP entries, `quick-xml` for
//! the slide XML), so no new crate family is introduced. Rendering targets
//! a read-only `GtkTextView` with text tags (SF Pro Display, large bold
//! slide title), reusing the Markdown tag pattern from `src/markdown.rs`.
//! Slide navigation mirrors the PDF page toolbar pattern from
//! `src/views/preview.rs`. Presentations are never editable and never
//! saved.

use std::path::Path;

use gtk::prelude::*;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Max decompressed XML parsed per ZIP entry (same budget as text files).
const MAX_PART_BYTES: u64 = crate::model::MAX_TEXT_BYTES;

/// Max slides parsed per deck; longer decks stop with `truncated` set so
/// huge decks stay cheap to display.
const MAX_SLIDES: usize = 500;

/// Max rendered blocks per slide; longer slides stop collecting with the
/// deck `truncated` flag set.
const MAX_BLOCKS_PER_SLIDE: usize = 2000;

/// One inline run of text with bold/italic flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
  /// Visible text of the run.
  pub text: String,
  /// Bold flag (`a:rPr b="1"`, `fo:font-weight="bold"`).
  pub bold: bool,
  /// Italic flag (`a:rPr i="1"`, `fo:font-style="italic"`).
  pub italic: bool,
}

/// One rendered block of a slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlideBlock {
  /// Normal body paragraph plus inline runs.
  Paragraph(Vec<Span>),
  /// Bulleted list item with its outline level (0 is top level).
  Bullet {
    /// Outline level (`a:pPr lvl`, ODP list nesting depth).
    level: u8,
    /// Inline runs of the item.
    spans: Vec<Span>,
  },
  /// Table as a plain grid: rows of plain-text cells.
  Table(Vec<Vec<String>>),
}

/// One slide: title plus body blocks plus speaker notes.
#[derive(Clone, Debug)]
pub struct Slide {
  /// Title runs (empty when the slide has no title placeholder).
  pub title: Vec<Span>,
  /// Body blocks in slide order.
  pub blocks: Vec<SlideBlock>,
  /// Speaker notes as plain paragraphs (empty when none exist).
  pub notes: Vec<String>,
}

/// A loaded presentation: slides in presentation order plus file metadata.
#[derive(Clone, Debug)]
pub struct Deck {
  /// Short format label (`PPTX`, `ODP`, `PPT`).
  pub format: String,
  /// Slides in presentation order.
  pub slides: Vec<Slide>,
  /// File size in bytes.
  pub file_bytes: u64,
  /// True when the slide or block cap stopped the parse (rest is omitted).
  pub truncated: bool,
}

impl Deck {
  /// Number of slides in the deck.
  pub fn slide_count(&self) -> usize {
    self.slides.len()
  }

  /// Number of whitespace-separated words over titles, blocks and notes.
  pub fn word_count(&self) -> usize {
    self
      .slides
      .iter()
      .map(|slide| {
        let title: usize = slide.title.iter().map(|s| s.text.split_whitespace().count()).sum();
        let body: usize = slide
          .blocks
          .iter()
          .map(|block| match block {
            SlideBlock::Paragraph(spans) => {
              spans.iter().map(|s| s.text.split_whitespace().count()).sum::<usize>()
            }
            SlideBlock::Bullet { spans, .. } => {
              spans.iter().map(|s| s.text.split_whitespace().count()).sum::<usize>()
            }
            SlideBlock::Table(rows) => rows
              .iter()
              .map(|row| row.iter().map(|c| c.split_whitespace().count()).sum::<usize>())
              .sum::<usize>(),
          })
          .sum::<usize>();
        let notes: usize = slide.notes.iter().map(|n| n.split_whitespace().count()).sum();
        title + body + notes
      })
      .sum::<usize>()
  }
}

/// Load a presentation for read-only preview. Returns a human-readable
/// reason for missing files, directories, empty files, oversized files,
/// password-protected packages, legacy `.ppt` files and corrupt content.
/// Never panics on corrupt input.
pub fn load_presentation(path: &Path) -> Result<Deck, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if meta.len() == 0 {
    return Err("file is empty".to_string());
  }
  if meta.len() > crate::model::MAX_TEXT_BYTES {
    return Err("file is too large for the slide viewer (over 8 MiB)".to_string());
  }
  let bytes = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  let format = crate::model::presentation_format_label(&ext).to_string();
  if is_zip_magic(&bytes) {
    return load_zip_package(&bytes, format, meta.len());
  }
  if is_ole_magic(&bytes) {
    return Err(ole_hint(&ext));
  }
  Err("file is not a recognized presentation (not a PowerPoint, ODP or legacy PowerPoint file)"
    .to_string())
}

/// Hint for OLE compound files: password-protected OOXML packages use OLE
/// too, so `.pptx`/`.odp` extensions report the password hint while `.ppt`
/// (and unknown extensions) report the legacy hint.
fn ole_hint(ext: &str) -> String {
  if ext == "pptx" || ext == "odp" {
    "file is encrypted (password-protected); open it without a password is not supported"
      .to_string()
  } else {
    "legacy .ppt format is not parsed (this includes password-protected files); convert it to .pptx to preview it"
      .to_string()
  }
}

fn is_zip_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04])
}

fn is_ole_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
}

/// Load a ZIP-based package: route to the `.pptx` or `.odp` parser by its
/// entries, or report an honest hint for other archives (spreadsheets,
/// word documents, plain zips).
fn load_zip_package(bytes: &[u8], format: String, file_bytes: u64) -> Result<Deck, String> {
  let entries = central_dir(bytes)?;
  let has = |name: &str| entries.iter().any(|e| e.name == name);
  if has("ppt/presentation.xml") {
    return load_pptx(&entries, bytes, format, file_bytes);
  }
  if has("xl/workbook.xml") {
    return Err("this is a spreadsheet, only presentations (.pptx) are supported".to_string());
  }
  if has("word/document.xml") {
    return Err("this is a word document, only presentations (.pptx) are supported".to_string());
  }
  if has("content.xml") {
    let xml = extract(&entries, bytes, "content.xml")?;
    if contains_draw_page(&xml) {
      let (slides, truncated) = parse_odp_xml(&xml);
      if slides.is_empty() {
        return Err("presentation has no readable slides".to_string());
      }
      return Ok(Deck { format, slides, file_bytes, truncated });
    }
    if contains_office_text(&xml) {
      return Err("this is a word document, only presentations (.pptx) are supported".to_string());
    }
    return Err("ZIP archive holds no presentation (missing ppt/presentation.xml)".to_string());
  }
  Err("ZIP archive holds no presentation (missing ppt/presentation.xml)".to_string())
}

/// True when the ODP `content.xml` holds at least one `draw:page` element
/// (checked on the raw bytes so the XML never needs parsing for routing).
fn contains_draw_page(xml: &[u8]) -> bool {
  find_subslice(xml, b"draw:page").is_some() || find_subslice(xml, b"draw:page ").is_some()
}

/// True when the XML holds an OpenDocument text body (an ODT package
/// misnamed as a presentation).
fn contains_office_text(xml: &[u8]) -> bool {
  find_subslice(xml, b"office:text").is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  if needle.len() > haystack.len() {
    return None;
  }
  haystack.windows(needle.len()).position(|window| window == needle)
}

/// Load a `.pptx` package: slide order comes from `ppt/presentation.xml`
/// via the relationship targets in `ppt/_rels/presentation.xml.rels`; when
/// the relationships part is missing, `ppt/slides/slide*.xml` entries are
/// used in sorted name order instead.
fn load_pptx(
  entries: &[ZipEntry],
  bytes: &[u8],
  format: String,
  file_bytes: u64,
) -> Result<Deck, String> {
  let order_xml = extract(entries, bytes, "ppt/presentation.xml")?;
  let rels = extract_optional(entries, bytes, "ppt/_rels/presentation.xml.rels");
  let mut order = slide_order(&order_xml, rels.as_deref(), entries);
  if order.is_empty() {
    return Err("presentation has no readable slides".to_string());
  }
  let mut truncated = false;
  if order.len() > MAX_SLIDES {
    order.truncate(MAX_SLIDES);
    truncated = true;
  }
  let mut slides = Vec::with_capacity(order.len());
  for name in &order {
    let xml = extract(entries, bytes, name)?;
    let (title, mut blocks, slide_capped) = parse_slide_xml(&xml);
    if slide_capped {
      truncated = true;
    }
    if blocks.len() > MAX_BLOCKS_PER_SLIDE {
      blocks.truncate(MAX_BLOCKS_PER_SLIDE);
      truncated = true;
    }
    let notes = load_notes_for(entries, bytes, name);
    slides.push(Slide { title, blocks, notes });
  }
  Ok(Deck { format, slides, file_bytes, truncated })
}

/// Best-effort speaker notes for one slide: `ppt/slides/slideN.xml` maps
/// to `ppt/notesSlides/notesSlideN.xml` by its trailing number. Missing or
/// unreadable notes stay empty instead of failing the deck.
fn load_notes_for(entries: &[ZipEntry], bytes: &[u8], slide_name: &str) -> Vec<String> {
  let number = slide_name
    .trim_end_matches(".xml")
    .rsplit(|c: char| !c.is_ascii_digit())
    .next()
    .unwrap_or("");
  if number.is_empty() {
    return Vec::new();
  }
  let notes_name = format!("ppt/notesSlides/notesSlide{number}.xml");
  let Ok(xml) = extract(entries, bytes, &notes_name) else {
    return Vec::new();
  };
  parse_notes_xml(&xml)
}

/// Resolve the slide order: `sldId` relationship ids from the presentation
/// part mapped through the relationships part to `ppt/...` entry names.
/// Falls back to sorted `ppt/slides/slide*.xml` entry names when the
/// relationships part is missing or names no usable target.
fn slide_order(order_xml: &[u8], rels: Option<&[u8]>, entries: &[ZipEntry]) -> Vec<String> {
  let ids = presentation_slide_ids(order_xml);
  if !ids.is_empty() {
    if let Some(rels_xml) = rels {
      let targets = rel_targets(rels_xml);
      let mut order = Vec::new();
      for id in &ids {
        if let Some(target) = targets.get(id) {
          let name = rel_target_name(target);
          if entries.iter().any(|e| e.name == name) {
            order.push(name);
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
    .filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))
    .collect();
  fallback.sort();
  fallback
}

/// Collect `sldId` relationship ids (`r:id`) in presentation order.
fn presentation_slide_ids(xml: &[u8]) -> Vec<String> {
  let mut reader = Reader::from_reader(xml);
  let mut ids = Vec::new();
  loop {
    match reader.read_event() {
      Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
        if local(e.name().into_inner()) == b"sldId" {
          if let Some(id) = attr_value(e, b"id") {
            ids.push(id);
          }
        }
      }
      Ok(Event::Eof) | Err(_) => break,
      _ => {}
    }
  }
  ids
}

/// Map relationship `Id` to `Target` for slide relationships.
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

/// Turn a relationship target (`slides/slide1.xml`, `/ppt/slides/...` or
/// an absolute URI) into a ZIP entry name. External targets (http, mailto,
/// custom protocols) resolve to an empty name that never matches an entry.
fn rel_target_name(target: &str) -> String {
  if target.contains("://") || target.starts_with("mailto:") {
    return String::new();
  }
  let trimmed = target.trim_start_matches('/');
  if trimmed.starts_with("ppt/") {
    trimmed.to_string()
  } else {
    format!("ppt/{trimmed}")
  }
}

/// One finished paragraph of a slide shape: level, bullet flag and runs.
struct FinishedPara {
  level: u8,
  is_bullet: bool,
  spans: Vec<Span>,
}

/// Collector for one DrawingML paragraph (`a:p`): outline level, bullet
/// flag and runs.
#[derive(Default)]
struct SlidePara {
  level: u8,
  is_bullet: bool,
  spans: Vec<Span>,
  text: String,
  bold: bool,
  italic: bool,
}

impl SlidePara {
  fn flush_run(&mut self) {
    if self.text.is_empty() {
      return;
    }
    self.spans.push(Span {
      text: std::mem::take(&mut self.text),
      bold: self.bold,
      italic: self.italic,
    });
  }

  fn finish(mut self) -> Option<FinishedPara> {
    self.flush_run();
    if !self.spans.iter().any(|s| !s.text.is_empty()) {
      return None;
    }
    Some(FinishedPara { level: self.level, is_bullet: self.is_bullet, spans: self.spans })
  }
}

/// Parse one `ppt/slides/slideN.xml` part: title placeholder shapes plus
/// body paragraphs, bold/italic runs, bulleted lists with outline levels
/// and tables as a plain grid. Field codes (`a:fld`, e.g. slide numbers)
/// are skipped. Returns the title runs, the body blocks and true when the
/// per-slide block cap stopped the collection.
fn parse_slide_xml(xml: &[u8]) -> (Vec<Span>, Vec<SlideBlock>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut title: Vec<Span> = Vec::new();
  let mut blocks: Vec<SlideBlock> = Vec::new();
  let mut capped = false;
  let mut shape_open = false;
  let mut shape_is_title = false;
  let mut shape_paras: Vec<FinishedPara> = Vec::new();
  let mut para: Option<SlidePara> = None;
  // True inside `a:t`: only there is character data body text.
  let mut in_text_elem = false;
  let mut skip_depth: usize = 0;
  let mut depth: usize = 0;
  // Table state: only the outermost table builds a grid; nested tables
  // merge their text into the current cell.
  let mut table_depth: usize = 0;
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut cell_paras: Vec<String> = Vec::new();

  macro_rules! push_block {
    ($block:expr) => {
      if blocks.len() >= MAX_BLOCKS_PER_SLIDE {
        capped = true;
      } else {
        blocks.push($block);
      }
    };
  }

  // Finish one paragraph: table cells collect plain text, shape paragraphs
  // are buffered until the shape end decides title versus body, stray
  // paragraphs (outside any shape) become body blocks directly.
  macro_rules! finish_para {
    () => {
      if let Some(p) = para.take() {
        if let Some(done) = p.finish() {
          if table_depth > 0 {
            let text: String =
              done.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
            if !text.trim().is_empty() {
              cell_paras.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
            }
          } else if shape_open {
            shape_paras.push(done);
          } else if done.is_bullet {
            push_block!(SlideBlock::Bullet { level: done.level, spans: done.spans });
          } else {
            push_block!(SlideBlock::Paragraph(done.spans));
          }
        }
      }
    };
  }

  // Finish one shape: title placeholders feed the slide title (the first
  // non-empty one wins), all other shapes become body blocks in encounter
  // order.
  macro_rules! finish_shape {
    () => {
      if shape_is_title && !title.iter().any(|s| !s.text.is_empty()) {
        for done in shape_paras.drain(..) {
          if !done.spans.iter().any(|s| !s.text.is_empty()) {
            continue;
          }
          if title.iter().any(|s| !s.text.is_empty()) {
            title.push(Span { text: " ".to_string(), bold: false, italic: false });
          }
          title.extend(done.spans);
        }
      } else {
        for done in shape_paras.drain(..) {
          if done.is_bullet {
            push_block!(SlideBlock::Bullet { level: done.level, spans: done.spans });
          } else {
            push_block!(SlideBlock::Paragraph(done.spans));
          }
        }
      }
      shape_paras.clear();
    };
  }

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
          b"fld" => skip_depth = depth,
          b"sp" => {
            shape_open = true;
            shape_is_title = false;
            shape_paras.clear();
          }
          b"ph" => {
            if shape_open {
              if let Some(kind) = attr_value(e, b"type") {
                let lower = kind.to_lowercase();
                if lower == "title" || lower == "ctrtitle" {
                  shape_is_title = true;
                }
              }
            }
          }
          b"p" => {
            para = Some(SlidePara::default());
          }
          b"pPr" => {
            if let Some(p) = para.as_mut() {
              if let Some(lvl) = attr_value(e, b"lvl") {
                p.level = lvl.parse::<u8>().unwrap_or(0).min(8);
              }
            }
          }
          b"buChar" | b"buAutoNum" | b"buBlip" => {
            if let Some(p) = para.as_mut() {
              p.is_bullet = true;
            }
          }
          b"buNone" => {
            if let Some(p) = para.as_mut() {
              p.is_bullet = false;
            }
          }
          b"r" => {
            if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = false;
              p.italic = false;
            }
          }
          b"rPr" => {
            if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = attr_on(e, b"b");
              p.italic = attr_on(e, b"i");
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
          b"ph" => {
            if shape_open {
              if let Some(kind) = attr_value(e, b"type") {
                let lower = kind.to_lowercase();
                if lower == "title" || lower == "ctrtitle" {
                  shape_is_title = true;
                }
              }
            }
          }
          b"pPr" => {
            if let Some(p) = para.as_mut() {
              if let Some(lvl) = attr_value(e, b"lvl") {
                p.level = lvl.parse::<u8>().unwrap_or(0).min(8);
              }
            }
          }
          b"buChar" | b"buAutoNum" | b"buBlip" => {
            if let Some(p) = para.as_mut() {
              p.is_bullet = true;
            }
          }
          b"buNone" => {
            if let Some(p) = para.as_mut() {
              p.is_bullet = false;
            }
          }
          b"rPr" => {
            if let Some(p) = para.as_mut() {
              p.flush_run();
              p.bold = attr_on(e, b"b");
              p.italic = attr_on(e, b"i");
            }
          }
          b"br" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\n');
            }
          }
          b"tab" => {
            if let Some(p) = para.as_mut() {
              p.text.push('\t');
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
          b"p" => finish_para!(),
          b"sp" => {
            finish_shape!();
            shape_open = false;
            shape_is_title = false;
          }
          b"t" => in_text_elem = false,
          b"tc" => {
            if table_depth == 1 {
              row.push(cell_paras.join(" "));
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
              push_block!(SlideBlock::Table(std::mem::take(&mut rows)));
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
  finish_para!();
  if shape_open {
    finish_shape!();
  }
  if table_depth > 0 && !rows.is_empty() && blocks.len() < MAX_BLOCKS_PER_SLIDE {
    blocks.push(SlideBlock::Table(std::mem::take(&mut rows)));
  }
  (title, blocks, capped)
}

/// Parse one notes slide (`ppt/notesSlides/notesSlideN.xml`) into plain
/// paragraphs: every text paragraph becomes one notes line, empty ones are
/// skipped.
fn parse_notes_xml(xml: &[u8]) -> Vec<String> {
  let mut reader = Reader::from_reader(xml);
  let mut notes = Vec::new();
  let mut current = String::new();
  let mut in_para = false;
  let mut in_text_elem = false;
  loop {
    match reader.read_event() {
      Ok(Event::Eof) => break,
      Err(_) => break,
      Ok(Event::Start(ref e)) => {
        match local(e.name().into_inner()) {
          b"p" => {
            in_para = true;
            current.clear();
          }
          b"t" => in_text_elem = true,
          _ => {}
        }
      }
      Ok(Event::End(ref e)) => {
        match local(e.name().into_inner()) {
          b"p" => {
            in_para = false;
            let line = current.split_whitespace().collect::<Vec<_>>().join(" ");
            if !line.is_empty() && notes.len() < MAX_BLOCKS_PER_SLIDE {
              notes.push(line);
            }
            current.clear();
          }
          b"t" => in_text_elem = false,
          _ => {}
        }
      }
      Ok(Event::Text(ref e)) => {
        if in_para && in_text_elem {
          current.push_str(&text_content(e));
        }
      }
      Ok(Event::CData(ref e)) => {
        if in_para && in_text_elem {
          current.push_str(&String::from_utf8_lossy(e));
        }
      }
      _ => {}
    }
  }
  notes
}

/// Parse `content.xml` of an ODP package into slides in `draw:page` order:
/// the first `text:h` of a page becomes the slide title, other headings
/// and paragraphs become body text, bold/italic come from automatic
/// styles, nested `text:list` items become bullets with their nesting
/// level, `table:table` builds grid rows and `presentation:notes` pages
/// become dim speaker notes. Returns the slides plus the truncation flag.
fn parse_odp_xml(xml: &[u8]) -> (Vec<Slide>, bool) {
  let mut reader = Reader::from_reader(xml);
  let mut slides: Vec<Slide> = Vec::new();
  let mut truncated = false;
  // Automatic style name mapped to (bold, italic).
  let mut styles: std::collections::HashMap<String, (bool, bool)> = std::collections::HashMap::new();
  let mut current_style: Option<String> = None;
  let mut style_bold = false;
  let mut style_italic = false;
  let mut slide: Option<OdpSlide> = None;
  let mut para: Option<SlidePara> = None;
  let mut para_is_title = false;
  let mut in_notes = false;
  // Nesting depth of `draw:page` elements: only the outermost page starts
  // a slide, so pages nested inside notes never split the slide.
  let mut page_depth: usize = 0;
  let mut list_depth: usize = 0;
  // Span stack for nested `text:span` elements (outer, ...).
  let mut span_stack: Vec<(bool, bool)> = Vec::new();
  let mut table_depth: usize = 0;
  let mut rows: Vec<Vec<String>> = Vec::new();
  let mut row: Vec<String> = Vec::new();
  let mut cell_paras: Vec<String> = Vec::new();
  let mut capped_any = false;

  macro_rules! push_slide {
    ($slide:expr) => {
      if slides.len() >= MAX_SLIDES {
        truncated = true;
      } else {
        slides.push($slide);
      }
    };
  }

  let finish_para = |para: &mut SlidePara,
                     is_title: bool,
                     in_list: bool,
                     level: u8,
                     in_notes: bool,
                     slide: &mut OdpSlide,
                     capped: &mut bool,
                     cell_paras: &mut Vec<String>,
                     table_depth: usize| {
    para.flush_run();
    if table_depth > 0 {
      let text: String = para.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
      if !text.trim().is_empty() {
        cell_paras.push(text.split_whitespace().collect::<Vec<_>>().join(" "));
      }
    } else if in_notes {
      // Notes text stays notes, even inside lists (never bullets).
      let text: String = para.spans.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join("");
      let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
      if !line.is_empty() {
        slide.notes.push(line);
      }
    } else if is_title && !slide.title_set {
      if para.spans.iter().any(|s| !s.text.is_empty()) {
        slide.title.extend(std::mem::take(&mut para.spans));
        slide.title_set = true;
      }
    } else if in_list {
      if push_slide_block(slide, SlideBlock::Bullet { level, spans: std::mem::take(&mut para.spans) })
      {
        *capped = true;
      }
    } else if push_slide_block(slide, SlideBlock::Paragraph(std::mem::take(&mut para.spans))) {
      *capped = true;
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
          b"page" => {
            if page_depth == 0 {
              slide = Some(OdpSlide::default());
              in_notes = false;
            }
            page_depth += 1;
          }
          b"notes" => in_notes = true,
          b"p" => {
            let mut p = SlidePara::default();
            if let Some(style) = attr_value(e, b"style-name") {
              if let Some((bold, italic)) = styles.get(&style) {
                p.bold = *bold;
                p.italic = *italic;
              }
            }
            para = Some(p);
            para_is_title = false;
          }
          b"h" => {
            let mut p = SlidePara::default();
            if let Some(style) = attr_value(e, b"style-name") {
              if let Some((bold, italic)) = styles.get(&style) {
                p.bold = *bold;
                p.italic = *italic;
              }
            }
            para = Some(p);
            para_is_title = true;
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
          b"table-cell" | b"covered-table-cell" => {
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
          b"page" => {
            page_depth = page_depth.saturating_sub(1);
            if page_depth > 0 {
              continue;
            }
            if let Some(mut p) = para.take() {
              if let Some(s) = slide.as_mut() {
                let is_title = para_is_title;
                let level = list_depth.saturating_sub(1).min(8) as u8;
                let in_list = list_depth > 0;
                let notes_flag = in_notes;
                finish_para(
                  &mut p,
                  is_title,
                  in_list,
                  level,
                  notes_flag,
                  s,
                  &mut capped_any,
                  &mut cell_paras,
                  table_depth,
                );
              }
              para_is_title = false;
            }
            if let Some(finished) = slide.take() {
              push_slide!(finished.into_slide());
            }
            in_notes = false;
          }
          b"notes" => in_notes = false,
          b"p" | b"h" => {
            if let Some(mut p) = para.take() {
              if let Some(s) = slide.as_mut() {
                let is_title = para_is_title;
                let level = list_depth.saturating_sub(1).min(8) as u8;
                let in_list = list_depth > 0;
                let notes_flag = in_notes;
                finish_para(
                  &mut p,
                  is_title,
                  in_list,
                  level,
                  notes_flag,
                  s,
                  &mut capped_any,
                  &mut cell_paras,
                  table_depth,
                );
              }
              para_is_title = false;
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
          b"table-cell" | b"covered-table-cell" => {
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
              if let Some(s) = slide.as_mut() {
                if push_slide_block(s, SlideBlock::Table(std::mem::take(&mut rows))) {
                  capped_any = true;
                }
              }
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
  if slides.len() >= MAX_SLIDES {
    truncated = true;
  }
  (slides, truncated || capped_any)
}

/// Push a body block unless it holds no text at all. Returns true when the
/// per-slide block cap dropped the block.
fn push_slide_block(slide: &mut OdpSlide, block: SlideBlock) -> bool {
  let empty = match &block {
    SlideBlock::Paragraph(spans) | SlideBlock::Bullet { spans, .. } => {
      !spans.iter().any(|s| !s.text.is_empty())
    }
    SlideBlock::Table(rows) => rows.is_empty(),
  };
  if empty {
    return false;
  }
  if slide.blocks.len() >= MAX_BLOCKS_PER_SLIDE {
    return true;
  }
  slide.blocks.push(block);
  false
}

/// Builder for one ODP `draw:page` while its XML streams in.
#[derive(Default)]
struct OdpSlide {
  title: Vec<Span>,
  title_set: bool,
  blocks: Vec<SlideBlock>,
  notes: Vec<String>,
}

impl OdpSlide {
  fn into_slide(self) -> Slide {
    Slide { title: self.title, blocks: self.blocks, notes: self.notes }
  }
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
  bytes.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
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
    let name_bytes =
      bytes.get(name_start..name_end).ok_or_else(|| "ZIP directory is corrupt".to_string())?;
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
/// the same errors as `extract` (corrupt entries still fail the deck).
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
        return Err("presentation part is too large for the viewer (over 8 MiB)".to_string());
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
    .map_err(|e| format!("cannot decompress presentation part: {e}"))?;
  if out.len() as u64 > MAX_PART_BYTES {
    return Err("presentation part is too large for the viewer (over 8 MiB)".to_string());
  }
  Ok(out)
}

/// Strip a namespace prefix (`a:p` becomes `p`).
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

/// True when a boolean DrawingML attribute (`b`, `i`) is switched on:
/// `1`, `true` or `on` (case-insensitive); missing or any other value
/// means off.
fn attr_on(tag: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> bool {
  match attr_value(tag, name) {
    None => false,
    Some(value) => {
      let lower = value.to_lowercase();
      lower == "1" || lower == "true" || lower == "on"
    }
  }
}

/// Decoded text content of a text event (entities unescaped, lossy fallback).
fn text_content(event: &quick_xml::events::BytesText<'_>) -> String {
  event.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| {
    String::from_utf8_lossy(event).into_owned()
  })
}

/// Render one slide of a loaded deck into a read-only `GtkTextView` buffer
/// with text tags (SF Pro Display, large bold title), following the
/// Markdown tag pattern from `src/markdown.rs`. Bullets render with their
/// outline indent plus a dim bullet marker, tables as plain `a | b` grid
/// rows, speaker notes as a dim `pptx.notes` section. A truncated deck ends
/// its last slide with a dim `pptx.truncated` hint line.
pub fn render_slide_into(buffer: &gtk::TextBuffer, deck: &Deck, index: usize) {
  ensure_tags(buffer);
  buffer.set_text("");
  let Some(slide) = deck.slides.get(index) else {
    return;
  };
  let mut iter = buffer.start_iter();
  if slide.title.iter().any(|s| !s.text.is_empty()) {
    insert_spans(&mut iter, &slide.title, &["h1"]);
    insert_with(&mut iter, "\n", &[]);
  }
  for block in &slide.blocks {
    match block {
      SlideBlock::Paragraph(spans) => {
        insert_spans(&mut iter, spans, &[]);
        insert_with(&mut iter, "\n", &[]);
      }
      SlideBlock::Bullet { level, spans } => {
        let indent = "  ".repeat((*level).min(8) as usize);
        insert_with(&mut iter, &indent, &[]);
        insert_with(&mut iter, "• ", &["dim"]);
        insert_spans(&mut iter, spans, &[]);
        insert_with(&mut iter, "\n", &[]);
      }
      SlideBlock::Table(rows) => {
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
  if !slide.notes.is_empty() {
    insert_with(&mut iter, "\n", &[]);
    insert_with(&mut iter, &crate::lang::t("pptx.notes"), &["dim"]);
    insert_with(&mut iter, "\n", &[]);
    for note in &slide.notes {
      insert_with(&mut iter, note, &["dim"]);
      insert_with(&mut iter, "\n", &[]);
    }
  }
  if deck.truncated && index + 1 >= deck.slides.len() {
    insert_with(
      &mut iter,
      &crate::lang::t_with("pptx.truncated", &[("count", &deck.slides.len().to_string())]),
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

  fn write_temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("preview-pptx-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write test presentation");
    path
  }

  const SAMPLE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId2" Type="slide" Target="slides/slide2.xml"/>
<Relationship Id="rId3" Type="slide" Target="slides/slide1.xml"/>
</Relationships>"#;

  // Presentation order names slide2 first even though slide1 sorts first.
  const SAMPLE_ORDER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<p:sldIdLst>
<p:sldId r:id="rId2"/>
<p:sldId r:id="rId3"/>
</p:sldIdLst>
</p:presentation>"#;

  const SAMPLE_SLIDE2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><a:ph type="title"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>Second First</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"#;

  const SAMPLE_SLIDE1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree>
<p:sp><p:nvSpPr><p:nvPr><a:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
<p:txBody><a:p><a:r><a:t>Welcome</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:nvSpPr><p:nvPr/></p:nvSpPr>
<p:txBody>
<a:p><a:r><a:rPr b="1"/><a:t>Bold</a:t></a:r><a:r><a:rPr i="1"/><a:t>Italic</a:t></a:r><a:r><a:t> plain &amp; done</a:t></a:r></a:p>
<a:p><a:pPr lvl="0"><a:buChar char="•"/></a:pPr><a:r><a:t>Top point</a:t></a:r></a:p>
<a:p><a:pPr lvl="2"><a:buAutoNum/></a:pPr><a:r><a:t>Nested point</a:t></a:r></a:p>
</p:txBody></p:sp>
<p:graphicFrame><a:tbl><a:tr><a:tc><a:txBody><a:p><a:r><a:t>A1</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>B1</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></p:graphicFrame>
</p:spTree></p:cSld></p:sld>"#;

  const SAMPLE_NOTES1: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
<p:cSld><p:spTree>
<p:sp><p:txBody><a:p><a:r><a:t>Say hello loudly</a:t></a:r></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:notes>"#;

  const SAMPLE_ODP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0">
<office:automatic-styles><style:style style:name="T1"><style:text-properties fo:font-weight="bold" fo:font-style="italic"/></style:style></office:automatic-styles>
<office:body><office:presentation>
<draw:page draw:name="page1">
<text:h>Opening</text:h>
<text:p>Plain <text:span text:style-name="T1">styled</text:span> tail</text:p>
<text:list><text:list-item><text:p>Point</text:p><text:list><text:list-item><text:p>Sub point</text:p></text:list-item></text:list></text:list-item></text:list>
<presentation:notes><draw:page><text:p>Remember this</text:p></draw:page></presentation:notes>
</draw:page>
<draw:page draw:name="page2">
<text:p>Lonely body</text:p>
</draw:page>
</office:presentation></office:body></office:document-content>"#;

  fn sample_pptx_zip() -> Vec<u8> {
    zip_store(&[
      ("[Content_Types].xml", b"<Types/>"),
      ("ppt/presentation.xml", SAMPLE_ORDER.as_bytes()),
      ("ppt/_rels/presentation.xml.rels", SAMPLE_RELS.as_bytes()),
      ("ppt/slides/slide1.xml", SAMPLE_SLIDE1.as_bytes()),
      ("ppt/slides/slide2.xml", SAMPLE_SLIDE2.as_bytes()),
      ("ppt/notesSlides/notesSlide1.xml", SAMPLE_NOTES1.as_bytes()),
    ])
  }

  #[test]
  fn pptx_sample_loads_in_presentation_order() {
    let path = write_temp("sample.pptx", &sample_pptx_zip());
    let deck = load_presentation(&path).expect("sample pptx loads");
    assert_eq!(deck.format, "PPTX");
    assert!(!deck.truncated);
    assert_eq!(deck.slide_count(), 2);
    // Presentation order names slide2 first (rels order wins over names).
    let first_title: String = deck.slides[0].title.iter().map(|s| s.text.clone()).collect();
    assert_eq!(first_title, "Second First");
    assert!(deck.slides[0].notes.is_empty());
    let second = &deck.slides[1];
    let title: String = second.title.iter().map(|s| s.text.clone()).collect();
    assert_eq!(title, "Welcome");
    assert_eq!(second.blocks.len(), 4);
    match &second.blocks[0] {
      SlideBlock::Paragraph(spans) => {
        assert_eq!(spans.len(), 3);
        assert!(spans[0].bold && !spans[0].italic);
        assert!(!spans[1].bold && spans[1].italic);
        assert_eq!(spans[2].text, " plain & done");
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    match &second.blocks[1] {
      SlideBlock::Bullet { level: 0, spans } => assert_eq!(spans[0].text, "Top point"),
      other => panic!("expected level 0 bullet, got {other:?}"),
    }
    match &second.blocks[2] {
      SlideBlock::Bullet { level: 2, spans } => assert_eq!(spans[0].text, "Nested point"),
      other => panic!("expected level 2 bullet, got {other:?}"),
    }
    match &second.blocks[3] {
      SlideBlock::Table(rows) => assert_eq!(rows, &vec![vec!["A1".to_string(), "B1".to_string()]]),
      other => panic!("expected table, got {other:?}"),
    }
    assert_eq!(second.notes, vec!["Say hello loudly".to_string()]);
    assert!(deck.word_count() > 0);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn pptx_falls_back_to_sorted_slides_without_rels() {
    let zip = zip_store(&[
      ("ppt/presentation.xml", b"<p:presentation/>"),
      ("ppt/slides/slide2.xml", SAMPLE_SLIDE2.as_bytes()),
      ("ppt/slides/slide1.xml", SAMPLE_SLIDE1.as_bytes()),
    ]);
    let path = write_temp("norels.pptx", &zip);
    let deck = load_presentation(&path).expect("pptx without rels loads");
    assert_eq!(deck.slide_count(), 2);
    let first_title: String = deck.slides[0].title.iter().map(|s| s.text.clone()).collect();
    assert_eq!(first_title, "Welcome");
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn odp_sample_loads() {
    let zip = zip_store(&[
      ("mimetype", b"application/vnd.oasis.opendocument.presentation"),
      ("content.xml", SAMPLE_ODP.as_bytes()),
    ]);
    let path = write_temp("sample.odp", &zip);
    let deck = load_presentation(&path).expect("sample odp loads");
    assert_eq!(deck.format, "ODP");
    assert_eq!(deck.slide_count(), 2);
    let first = &deck.slides[0];
    let title: String = first.title.iter().map(|s| s.text.clone()).collect();
    assert_eq!(title, "Opening");
    match &first.blocks[0] {
      SlideBlock::Paragraph(spans) => {
        assert_eq!(spans.len(), 3);
        assert!(spans[1].bold && spans[1].italic);
        assert_eq!(spans[1].text, "styled");
      }
      other => panic!("expected paragraph, got {other:?}"),
    }
    match &first.blocks[1] {
      SlideBlock::Bullet { level: 0, spans } => assert_eq!(spans[0].text, "Point"),
      other => panic!("expected level 0 bullet, got {other:?}"),
    }
    match &first.blocks[2] {
      SlideBlock::Bullet { level: 1, spans } => assert_eq!(spans[0].text, "Sub point"),
      other => panic!("expected level 1 bullet, got {other:?}"),
    }
    assert_eq!(first.notes, vec!["Remember this".to_string()]);
    // A page without a heading keeps an empty title but still counts.
    assert!(deck.slides[1].title.is_empty());
    assert_eq!(deck.slide_count(), 2);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn corrupt_presentations_hint_without_crash() {
    // Not a ZIP or OLE file.
    let plain = write_temp("bad.pptx", b"just some text");
    assert!(load_presentation(&plain).is_err());
    let _ = std::fs::remove_file(&plain);
    // Truncated ZIP.
    let broken = write_temp("broken.pptx", b"PK\x03\x04truncated");
    assert!(load_presentation(&broken).is_err());
    let _ = std::fs::remove_file(&broken);
    // ZIP without any presentation part.
    let empty_zip = write_temp("emptyzip.pptx", &zip_store(&[("readme.txt", b"hi")]));
    let err = load_presentation(&empty_zip).expect_err("no presentation part must fail");
    assert!(err.contains("no presentation"), "got {err:?}");
    let _ = std::fs::remove_file(&empty_zip);
    // Spreadsheet archive reports its kind honestly.
    let sheet = write_temp(
      "sheet.pptx",
      &zip_store(&[("xl/workbook.xml", b"<workbook/>")]),
    );
    let err = load_presentation(&sheet).expect_err("spreadsheet must fail");
    assert!(err.contains("spreadsheet"), "got {err:?}");
    let _ = std::fs::remove_file(&sheet);
    // Word archive reports its kind honestly.
    let word = write_temp(
      "word.pptx",
      &zip_store(&[("word/document.xml", b"<document/>")]),
    );
    let err = load_presentation(&word).expect_err("word document must fail");
    assert!(err.contains("word document"), "got {err:?}");
    let _ = std::fs::remove_file(&word);
    // ODT package misnamed as a presentation reports the word hint.
    let odt = write_temp(
      "text.pptx",
      &zip_store(&[("content.xml", b"<office:text/>")]),
    );
    let err = load_presentation(&odt).expect_err("odt content must fail");
    assert!(err.contains("word document"), "got {err:?}");
    let _ = std::fs::remove_file(&odt);
    // Legacy OLE reports the legacy hint, password-looking OLE the password hint.
    let ole = write_temp("legacy.ppt", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00]);
    let err = load_presentation(&ole).expect_err("legacy ppt must fail");
    assert!(err.contains("legacy .ppt"), "got {err:?}");
    let _ = std::fs::remove_file(&ole);
    let locked = write_temp(
      "locked.pptx",
      &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0x00],
    );
    let err = load_presentation(&locked).expect_err("locked pptx must fail");
    assert!(err.contains("password"), "got {err:?}");
    let _ = std::fs::remove_file(&locked);
  }

  #[test]
  fn rel_target_names_resolve() {
    assert_eq!(rel_target_name("slides/slide1.xml"), "ppt/slides/slide1.xml");
    assert_eq!(rel_target_name("/ppt/slides/slide1.xml"), "ppt/slides/slide1.xml");
    assert_eq!(rel_target_name("https://example.com/slide.xml"), "");
    assert_eq!(rel_target_name("mailto:deck@example.com"), "");
  }
}
