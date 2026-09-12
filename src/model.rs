//! File model for Preview: text detection, loading and saving plus PDF metadata.
//!
//! Supported text extensions are opened as editable text. Markdown gets a
//! rendered preview mode plus a raw edit mode. PDFs open read-only with
//! per-page text extraction (lazy: only the current page is extracted).
//! Images open read-only in a picture viewer (`gtk::Picture`): raster
//! formats are decoded with the pure-Rust `image` crate, SVG files are
//! handed to GTK (librsvg). Anything else is rejected as unsupported with
//! a hint (binary files are never loaded as text).

use std::path::{Path, PathBuf};

/// How Preview treats a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
  /// Plain editable text (`.txt`, `.py`, `.json`, ...).
  Text,
  /// Markdown: rendered preview plus raw edit mode.
  Markdown,
  /// PDF: read-only page viewer (previous/next, zoom, fit width).
  Pdf,
  /// Image: read-only picture viewer (zoom in/out, fit window).
  Image,
  /// Not supported yet (audio, video, office, binary).
  Unsupported,
}

/// Known plain-text extensions (lowercase, without dot).
pub const TEXT_EXTENSIONS: &[&str] = &[
  "txt", "md", "markdown", "json", "py", "rs", "toml", "yaml", "yml", "xml", "html", "htm",
  "css", "js", "ts", "sh", "bash", "zsh", "log", "ini", "cfg", "conf", "csv", "tsv", "sql",
  "c", "h", "cpp", "hpp", "java", "go", "rb", "php", "swift", "kt", "lua", "r", "pl", "tex",
];

/// Markdown extensions get the special rendered preview.
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown"];

/// PDF files open in the read-only page viewer.
pub const PDF_EXTENSIONS: &[&str] = &["pdf"];

/// Image files open in the read-only picture viewer (see `wiki/Image.md`).
/// HEIF/HEIC is included so such files land on the image page with a clear
/// reason hint when no decoder is available (neither the `image` crate nor
/// the system loaders decode HEIF here).
pub const IMAGE_EXTENSIONS: &[&str] = &[
  "png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "avif", "heif",
  "heic",
];

/// Max file size loaded as text in the basis version (8 MiB).
pub const MAX_TEXT_BYTES: u64 = 8 * 1024 * 1024;

/// Classify a path by extension only.
pub fn classify(path: &Path) -> FileKind {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  if MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
    return FileKind::Markdown;
  }
  if PDF_EXTENSIONS.contains(&ext.as_str()) {
    return FileKind::Pdf;
  }
  if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
    return FileKind::Image;
  }
  if TEXT_EXTENSIONS.contains(&ext.as_str()) {
    return FileKind::Text;
  }
  // No extension: try text fallback later; classify as text for now and
  // let the binary sniff decide.
  if ext.is_empty() {
    return FileKind::Text;
  }
  FileKind::Unsupported
}

/// Classify with a magic-byte fallback so misnamed files still open on the
/// right page: when the extension does not say image but the file header
/// carries known image magic, the file is treated as an image. Other kinds
/// are never changed by sniffing.
pub fn classify_file(path: &Path) -> FileKind {
  let by_ext = classify(path);
  if by_ext == FileKind::Image {
    return by_ext;
  }
  if read_prefix(path, 2048)
    .map(|bytes| sniff_image(&bytes))
    .unwrap_or(false)
  {
    return FileKind::Image;
  }
  by_ext
}

/// Read up to `max` leading bytes of a file for magic-byte sniffing.
fn read_prefix(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
  use std::io::Read;
  let mut file = std::fs::File::open(path)?;
  let mut buf = vec![0u8; max];
  let n = file.read(&mut buf)?;
  buf.truncate(n);
  Ok(buf)
}

/// Returns true when the bytes look like binary (NUL byte in prefix).
pub fn looks_binary(sample: &[u8]) -> bool {
  sample.contains(&0)
}

/// Returns true when the bytes carry a known image magic number or SVG
/// markup (see `wiki/Image.md` for the format table).
pub fn sniff_image(bytes: &[u8]) -> bool {
  is_png_magic(bytes)
    || is_jpeg_magic(bytes)
    || is_gif_magic(bytes)
    || is_bmp_magic(bytes)
    || is_webp_magic(bytes)
    || is_tiff_magic(bytes)
    || is_ico_magic(bytes)
    || is_iso_media_image(bytes)
    || is_svg_markup(bytes)
}

fn is_png_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
}

fn is_jpeg_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0xFF, 0xD8, 0xFF])
}

fn is_gif_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

fn is_bmp_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(b"BM")
}

fn is_webp_magic(bytes: &[u8]) -> bool {
  bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn is_tiff_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
}

fn is_ico_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[0x00, 0x00, 0x01, 0x00])
}

/// ISO base media (`ftyp`) brands used by AVIF and HEIF/HEIC still images.
fn is_iso_media_image(bytes: &[u8]) -> bool {
  if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
    return false;
  }
  let brand = &bytes[8..12];
  matches!(
    brand,
    b"avif" | b"avis" | b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"hevm"
      | b"hevs" | b"mif1" | b"msf1"
  )
}

/// SVG has no binary magic; detect `<svg` markup near the start of the
/// file (case-insensitive, allows a leading XML declaration). Only the
/// given prefix is scanned so deep matches do not count.
pub fn is_svg_markup(bytes: &[u8]) -> bool {
  let end = bytes.len().min(2048);
  let lower: Vec<u8> = bytes[..end].iter().map(|b| b.to_ascii_lowercase()).collect();
  find_subslice(&lower, b"<svg").is_some()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack
    .windows(needle.len())
    .position(|window| window == needle)
}

/// Returns true for SVG files by extension or by markup sniffing, so the
/// viewer can pick the GTK (librsvg) render path.
pub fn is_svg_file(path: &Path) -> bool {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  if ext == "svg" {
    return true;
  }
  read_prefix(path, 2048)
    .map(|bytes| is_svg_markup(&bytes))
    .unwrap_or(false)
}

/// Max display size for images: the longest side is downscaled to this
/// many pixels so huge images stay cheap to render. The status line shows
/// the original dimensions plus a downscale note.
pub const MAX_IMAGE_DISPLAY: u32 = 2048;

/// Scale `(width, height)` down so the longest side fits
/// `MAX_IMAGE_DISPLAY`, preserving the aspect ratio. Smaller images keep
/// their size. Returns at least 1 pixel per side.
pub fn display_size(width: u32, height: u32) -> (u32, u32) {
  let longest = width.max(height);
  if longest <= MAX_IMAGE_DISPLAY || longest == 0 {
    return (width.max(1), height.max(1));
  }
  let scale = MAX_IMAGE_DISPLAY as f64 / longest as f64;
  (
    ((width as f64 * scale).round() as u32).max(1),
    ((height as f64 * scale).round() as u32).max(1),
  )
}

/// Read-only image metadata probed without loading full pixels.
#[derive(Clone, Copy, Debug)]
pub struct ImageMeta {
  /// Original pixel dimensions (`0 x 0` when unknown, e.g. SVG without size).
  pub width: u32,
  /// Original pixel height (`0` when unknown).
  pub height: u32,
  /// File size in bytes.
  pub file_bytes: u64,
  /// True when the display size was downscaled from the original.
  pub downscaled: bool,
  /// Pixels used for display (downscaled when huge).
  pub display_width: u32,
  /// Pixels used for display (downscaled when huge).
  pub display_height: u32,
  /// True for SVG files (rendered via GTK/librsvg, not the `image` crate).
  pub is_svg: bool,
}

impl ImageMeta {
  /// True when real dimensions are known.
  pub fn has_dimensions(&self) -> bool {
    self.width > 0 && self.height > 0
  }
}

/// Probe an image file: stat plus dimension lookup without decoding full
/// pixels. Returns a human-readable reason for missing files, directories
/// and undecodable content (corrupt files never crash, they hint).
pub fn probe_image(path: &Path) -> Result<ImageMeta, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if is_svg_file(path) {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read SVG: {e}"))?;
    let (width, height) = parse_svg_size(&text).unwrap_or((0, 0));
    let (display_width, display_height) = display_size(width, height);
    return Ok(ImageMeta {
      width,
      height,
      file_bytes: meta.len(),
      downscaled: false,
      display_width,
      display_height,
      is_svg: true,
    });
  }
  let (width, height) =
    image::image_dimensions(path).map_err(|e| format!("cannot decode image: {e}"))?;
  let (display_width, display_height) = display_size(width, height);
  Ok(ImageMeta {
    width,
    height,
    file_bytes: meta.len(),
    downscaled: display_width != width || display_height != height,
    display_width,
    display_height,
    is_svg: false,
  })
}

/// Parse an SVG canvas size in pixels: `width`/`height` attributes first
/// (an optional `px` suffix is accepted), then the `viewBox` size.
/// Returns `None` when no usable size is present.
pub fn parse_svg_size(text: &str) -> Option<(u32, u32)> {
  if let (Some(w), Some(h)) = (svg_attr_len(text, "width"), svg_attr_len(text, "height")) {
    if w > 0 && h > 0 {
      return Some((w, h));
    }
  }
  let view_box = svg_attr_raw(text, "viewBox")?;
  let parts: Vec<&str> = view_box
    .split(|c: char| c == ' ' || c == ',' || c == '\t' || c == '\n')
    .filter(|p| !p.is_empty())
    .collect();
  if parts.len() == 4 {
    let w = parts[2].parse::<f64>().ok()?;
    let h = parts[3].parse::<f64>().ok()?;
    if w > 0.0 && h > 0.0 {
      return Some((w.round() as u32, h.round() as u32));
    }
  }
  None
}

/// Read a raw `name="value"` attribute from SVG/XML text.
fn svg_attr_raw<'a>(text: &'a str, name: &str) -> Option<&'a str> {
  let key = format!("{name}=\"");
  let start = text.find(&key)? + key.len();
  let end = text[start..].find('"')?;
  Some(&text[start..start + end])
}

/// Parse an SVG length attribute (`640`, `640px`) to whole pixels.
fn svg_attr_len(text: &str, name: &str) -> Option<u32> {
  let raw = svg_attr_raw(text, name)?.trim();
  let digits: String = raw
    .chars()
    .take_while(|c| c.is_ascii_digit() || *c == '.')
    .collect();
  let value: f64 = digits.parse().ok()?;
  if value <= 0.0 {
    return None;
  }
  Some(value.round() as u32)
}

/// Min/max zoom factor for the image viewer.
pub const MIN_IMAGE_ZOOM: f64 = 0.1;
/// Max zoom factor for the image viewer.
pub const MAX_IMAGE_ZOOM: f64 = 8.0;
/// Zoom step per button press.
pub const IMAGE_ZOOM_STEP: f64 = 0.25;

/// Clamp a zoom factor into `MIN_IMAGE_ZOOM..=MAX_IMAGE_ZOOM`.
pub fn clamp_image_zoom(zoom: f64) -> f64 {
  zoom.clamp(MIN_IMAGE_ZOOM, MAX_IMAGE_ZOOM)
}

/// One zoom-in step, clamped.
pub fn image_zoom_in(zoom: f64) -> f64 {
  clamp_image_zoom(zoom + IMAGE_ZOOM_STEP)
}

/// One zoom-out step, clamped.
pub fn image_zoom_out(zoom: f64) -> f64 {
  clamp_image_zoom(zoom - IMAGE_ZOOM_STEP)
}

/// Scale base pixels by a zoom factor (at least 1 pixel per side).
pub fn zoomed_size(base_width: u32, base_height: u32, zoom: f64) -> (u32, u32) {
  let zoom = clamp_image_zoom(zoom);
  (
    ((base_width as f64 * zoom).round() as u32).max(1),
    ((base_height as f64 * zoom).round() as u32).max(1),
  )
}

/// Zoom factor that fits base pixels into a viewport, preserving aspect.
/// Returns `1.0` when any size is zero; the result is clamped to the zoom
/// range. The caller re-applies fit after resizes (cheap: press Fit again).
pub fn fit_zoom_for(base_width: u32, base_height: u32, view_width: i32, view_height: i32) -> f64 {
  if base_width == 0 || base_height == 0 || view_width <= 0 || view_height <= 0 {
    return 1.0;
  }
  let scale = (view_width as f64 / base_width as f64).min(view_height as f64 / base_height as f64);
  if !scale.is_finite() {
    return 1.0;
  }
  clamp_image_zoom(scale)
}

/// Format a byte count for the status line (`512 B`, `1.5 KB`, `5.0 MB`).
pub fn format_file_size(bytes: u64) -> String {
  const KB: f64 = 1024.0;
  const MB: f64 = 1024.0 * 1024.0;
  const GB: f64 = 1024.0 * 1024.0 * 1024.0;
  let size = bytes as f64;
  if size < KB {
    format!("{bytes} B")
  } else if size < MB {
    format!("{:.1} KB", size / KB)
  } else if size < GB {
    format!("{:.1} MB", size / MB)
  } else {
    format!("{:.1} GB", size / GB)
  }
}

/// Load a text file. Rejects oversized files, binary content and IO errors
/// with a human-readable reason.
pub fn load_text(path: &Path) -> Result<String, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if meta.len() > MAX_TEXT_BYTES {
    return Err("file is too large for the text basis (over 8 MiB)".to_string());
  }
  let bytes = std::fs::read(path).map_err(|e| format!("cannot read file: {e}"))?;
  let prefix = if bytes.len() > 8192 { &bytes[..8192] } else { &bytes[..] };
  if looks_binary(prefix) {
    return Err("file looks like binary data".to_string());
  }
  String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".to_string())
}

/// Save text back to the file.
pub fn save_text(path: &Path, content: &str) -> Result<(), String> {
  std::fs::write(path, content).map_err(|e| format!("cannot save file: {e}"))
}

/// Read-only PDF document with lazy per-page text extraction.
///
/// Only metadata (page table) is kept in memory; page text is extracted
/// on demand for the current page so huge page counts stay cheap.
pub struct PdfDoc {
  doc: lopdf::Document,
  pages: Vec<u32>,
}

impl PdfDoc {
  /// Open a PDF file. Returns a human-readable reason for missing files,
  /// directories, corrupt files and encrypted (password-protected) files.
  pub fn open(path: &Path) -> Result<Self, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
    if meta.is_dir() {
      return Err("path is a directory".to_string());
    }
    let doc = lopdf::Document::load(path).map_err(|e| pdf_reason(&e))?;
    if doc.is_encrypted() {
      return Err(
        "file is encrypted (password-protected); open it without a password is not supported"
          .to_string(),
      );
    }
    let mut pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
    pages.sort_unstable();
    if pages.is_empty() {
      return Err("file has no readable pages".to_string());
    }
    Ok(Self { doc, pages })
  }

  /// Number of pages in the document.
  pub fn page_count(&self) -> usize {
    self.pages.len()
  }

  /// Extract the text of one page (0-based index). Returns an empty string
  /// when the page holds no extractable text (e.g. scanned images).
  pub fn page_text(&self, index: usize) -> Result<String, String> {
    let page_no = *self
      .pages
      .get(index)
      .ok_or_else(|| format!("page {} is out of range", index + 1))?;
    self
      .doc
      .extract_text(&[page_no])
      .map_err(|e| pdf_reason(&e))
  }
}

/// Map a lopdf error to a short human-readable reason.
fn pdf_reason(err: &lopdf::Error) -> String {
  let text = err.to_string().to_lowercase();
  if text.contains("encrypt") || text.contains("password") || text.contains("permission") {
    return format!(
      "file is encrypted (password-protected); open it without a password is not supported ({err})"
    );
  }
  format!("cannot parse PDF: {err}")
}

/// Clamp a 0-based page index into `0..total`. Returns 0 when empty.
pub fn clamp_page(index: usize, total: usize) -> usize {
  if total == 0 {
    return 0;
  }
  index.min(total - 1)
}

/// Display name for a path (file name or full path fallback).
pub fn display_name(path: &Path) -> String {
  path
    .file_name()
    .and_then(|n| n.to_str())
    .map(|s| s.to_string())
    .unwrap_or_else(|| path.display().to_string())
}

/// Resolve the initial file from CLI args (`preview /path/to/file`).
/// Returns the first argument that exists on disk.
pub fn initial_file_from_args(args: &[String]) -> Option<PathBuf> {
  for arg in args.iter().skip(1) {
    if arg == "--" {
      continue;
    }
    if arg.starts_with('-') {
      continue;
    }
    let path = PathBuf::from(arg);
    if path.exists() {
      return Some(path);
    }
    // Return the path anyway so the UI can show a proper error.
    return Some(path);
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn markdown_classified() {
    assert_eq!(classify(Path::new("notes.md")), FileKind::Markdown);
    assert_eq!(classify(Path::new("README.MD")), FileKind::Markdown);
  }

  #[test]
  fn text_classified() {
    assert_eq!(classify(Path::new("main.py")), FileKind::Text);
    assert_eq!(classify(Path::new("data.json")), FileKind::Text);
    assert_eq!(classify(Path::new("notes.txt")), FileKind::Text);
  }

  #[test]
  fn pdf_classified() {
    assert_eq!(classify(Path::new("doc.pdf")), FileKind::Pdf);
    assert_eq!(classify(Path::new("SCAN.PDF")), FileKind::Pdf);
  }

  #[test]
  fn unsupported_classified() {
    assert_eq!(classify(Path::new("song.mp3")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("sheet.xlsx")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("movie.mp4")), FileKind::Unsupported);
  }

  #[test]
  fn image_extensions_classified() {
    for name in [
      "photo.png",
      "photo.PNG",
      "photo.jpg",
      "photo.JPG",
      "photo.jpeg",
      "anim.gif",
      "anim.GIF",
      "bitmap.bmp",
      "art.webp",
      "scan.tiff",
      "scan.TIF",
      "vector.svg",
      "icon.ico",
      "shot.avif",
      "phone.heic",
      "phone.heif",
    ] {
      assert_eq!(classify(Path::new(name)), FileKind::Image, "failed for {name}");
    }
  }

  #[test]
  fn image_magic_sniffed() {
    assert!(sniff_image(&[137, 80, 78, 71, 13, 10, 26, 10, 0]));
    assert!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
    assert!(sniff_image(b"GIF89a\x01\x00"));
    assert!(sniff_image(b"GIF87a\x01\x00"));
    assert!(sniff_image(b"BM\x36\x00"));
    assert!(sniff_image(b"RIFF\x00\x00\x00\x00WEBP"));
    assert!(sniff_image(&[0x49, 0x49, 0x2A, 0x00]));
    assert!(sniff_image(&[0x4D, 0x4D, 0x00, 0x2A]));
    assert!(sniff_image(&[0x00, 0x00, 0x01, 0x00, 0x01]));
    assert!(sniff_image(b"\x00\x00\x00\x20ftypavif\x00"));
    assert!(sniff_image(b"\x00\x00\x00\x20ftypheic\x00"));
    assert!(sniff_image(b"<?xml version=\"1.0\"?><svg width=\"1\">"));
    assert!(sniff_image(b"  <SVG viewBox=\"0 0 1 1\">"));
    assert!(!sniff_image(b"hello world"));
    assert!(!sniff_image(b"%PDF-1.4"));
    assert!(!sniff_image(b""));
  }

  #[test]
  fn misnamed_files_sniffed_as_image() {
    let dir = std::env::temp_dir().join("preview-image-test");
    let _ = std::fs::create_dir_all(&dir);
    // PNG magic without any extension still opens as an image.
    let no_ext = dir.join("misnamed-no-ext");
    std::fs::write(&no_ext, [137, 80, 78, 71, 13, 10, 26, 10, 0, 1, 2]).expect("write png magic");
    assert_eq!(classify_file(&no_ext), FileKind::Image);
    // JPEG magic behind a `.txt` extension still opens as an image.
    let txt_ext = dir.join("misnamed.txt");
    std::fs::write(&txt_ext, [0xFF, 0xD8, 0xFF, 0xE0, 0, 1, 2]).expect("write jpeg magic");
    assert_eq!(classify_file(&txt_ext), FileKind::Image);
    // Plain text is unaffected by sniffing.
    let plain = dir.join("notes.txt");
    std::fs::write(&plain, b"just some words").expect("write text");
    assert_eq!(classify_file(&plain), FileKind::Text);
    let _ = std::fs::remove_file(&no_ext);
    let _ = std::fs::remove_file(&txt_ext);
    let _ = std::fs::remove_file(&plain);
  }

  #[test]
  fn page_index_clamping() {
    assert_eq!(clamp_page(0, 0), 0);
    assert_eq!(clamp_page(5, 0), 0);
    assert_eq!(clamp_page(0, 3), 0);
    assert_eq!(clamp_page(2, 3), 2);
    assert_eq!(clamp_page(3, 3), 2);
    assert_eq!(clamp_page(99, 10), 9);
  }

  #[test]
  fn pdf_missing_file_reports_reason() {
    let result = PdfDoc::open(Path::new("/nonexistent-preview-test/missing.pdf"));
    assert!(result.is_err());
  }

  #[test]
  fn pdf_corrupt_file_reports_reason() {
    let dir = std::env::temp_dir().join("preview-pdf-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("corrupt.pdf");
    std::fs::write(&path, b"%PDF-1.4 broken (((\x00\x01\x02").expect("write corrupt pdf");
    let result = PdfDoc::open(&path);
    assert!(result.is_err());
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn pdf_single_page_text_extraction() {
    let dir = std::env::temp_dir().join("preview-pdf-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("hello.pdf");
    std::fs::write(&path, minimal_pdf(b"Hello PDF")).expect("write minimal pdf");
    let doc = PdfDoc::open(&path).expect("open minimal pdf");
    assert_eq!(doc.page_count(), 1);
    let text = doc.page_text(0).expect("extract page text");
    assert!(text.contains("Hello PDF"), "unexpected page text: {text:?}");
    assert!(doc.page_text(1).is_err());
    let _ = std::fs::remove_file(&path);
  }

  /// Build a minimal valid single-page PDF with correct xref offsets.
  fn minimal_pdf(line: &[u8]) -> Vec<u8> {
    let stream = [b"BT /F1 12 Tf 10 100 Td (".as_slice(), line, b") Tj ET".as_slice()].concat();
    let objects: Vec<Vec<u8>> = vec![
      b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
      b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
      [
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".as_slice(),
      ]
      .concat(),
      [
        format!("<< /Length {} >>\nstream\n", stream.len()).as_bytes(),
        &stream,
        b"\nendstream",
      ]
      .concat(),
      b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
      offsets.push(out.len());
      out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
      out.extend_from_slice(body);
      out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
      out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
      format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
      )
      .as_bytes(),
    );
    out
  }

  #[test]
  fn binary_sniff() {
    assert!(looks_binary(b"hello\x00world"));
    assert!(!looks_binary(b"hello world\n"));
  }

  #[test]
  fn svg_size_parsing() {
    assert_eq!(parse_svg_size(r#"<svg width="640" height="480">"#,), Some((640, 480)));
    assert_eq!(
      parse_svg_size(r#"<svg width="640px" height="480px">"#),
      Some((640, 480))
    );
    assert_eq!(
      parse_svg_size(r#"<svg viewBox="0 0 320 200">"#),
      Some((320, 200))
    );
    assert_eq!(parse_svg_size(r#"<svg>"#), None);
    assert_eq!(parse_svg_size(r#"<svg width="0" height="10">"#,), None);
  }

  #[test]
  fn display_size_downscales_huge_images() {
    assert_eq!(display_size(800, 600), (800, 600));
    assert_eq!(display_size(2048, 100), (2048, 100));
    assert_eq!(display_size(8000, 4000), (2048, 1024));
    assert_eq!(display_size(100, 8000), (26, 2048));
    assert_eq!(display_size(0, 0), (1, 1));
  }

  #[test]
  fn image_zoom_steps_and_clamps() {
    assert_eq!(image_zoom_in(1.0), 1.25);
    assert_eq!(image_zoom_out(1.0), 0.75);
    assert_eq!(image_zoom_in(MAX_IMAGE_ZOOM), MAX_IMAGE_ZOOM);
    assert_eq!(image_zoom_out(MIN_IMAGE_ZOOM), MIN_IMAGE_ZOOM);
    assert_eq!(clamp_image_zoom(99.0), MAX_IMAGE_ZOOM);
    assert_eq!(clamp_image_zoom(-1.0), MIN_IMAGE_ZOOM);
    assert_eq!(zoomed_size(800, 600, 2.0), (1600, 1200));
    assert_eq!(zoomed_size(1, 1, 0.05), (1, 1));
  }

  #[test]
  fn fit_zoom_computation() {
    assert_eq!(fit_zoom_for(800, 600, 400, 300), 0.5);
    assert_eq!(fit_zoom_for(800, 600, 1600, 1200), 2.0);
    assert_eq!(fit_zoom_for(0, 600, 400, 300), 1.0);
    assert_eq!(fit_zoom_for(800, 600, 0, 0), 1.0);
    assert_eq!(fit_zoom_for(100000, 1, 10, 10), MIN_IMAGE_ZOOM);
  }

  #[test]
  fn file_size_formatting() {
    assert_eq!(format_file_size(0), "0 B");
    assert_eq!(format_file_size(512), "512 B");
    assert_eq!(format_file_size(1024), "1.0 KB");
    assert_eq!(format_file_size(1536), "1.5 KB");
    assert_eq!(format_file_size(5 * 1024 * 1024), "5.0 MB");
  }

  #[test]
  fn probe_image_roundtrip() {
    let dir = std::env::temp_dir().join("preview-image-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tiny.png");
    let img = image::RgbImage::from_pixel(3, 2, image::Rgb([10, 20, 30]));
    img.save(&path).expect("write tiny png");
    let meta = probe_image(&path).expect("probe tiny png");
    assert_eq!((meta.width, meta.height), (3, 2));
    assert!(!meta.downscaled);
    assert!(!meta.is_svg);
    assert!(meta.file_bytes > 0);
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn probe_image_reports_reasons() {
    assert!(probe_image(Path::new("/nonexistent-preview-test/missing.png")).is_err());
    let dir = std::env::temp_dir().join("preview-image-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("corrupt.png");
    std::fs::write(&path, b"definitely not image bytes at all").expect("write corrupt png");
    let err = probe_image(&path).expect_err("corrupt must fail");
    assert!(err.contains("cannot decode image"), "unexpected reason: {err}");
    let svg_path = dir.join("tiny.svg");
    std::fs::write(&svg_path, b"<svg width=\"16\" height=\"9\"></svg>").expect("write svg");
    let svg = probe_image(&svg_path).expect("probe svg");
    assert!(svg.is_svg);
    assert_eq!((svg.width, svg.height), (16, 9));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&svg_path);
  }
}
