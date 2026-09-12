//! File model for Preview: text detection, loading and saving plus PDF metadata.
//!
//! Supported text extensions are opened as editable text. Markdown gets a
//! rendered preview mode plus a raw edit mode. PDFs open read-only with
//! per-page text extraction (lazy: only the current page is extracted).
//! Anything else is rejected as unsupported with a hint (binary files are
//! never loaded as text).

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
  /// Not supported yet (images, audio, video, office, binary).
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

/// Returns true when the bytes look like binary (NUL byte in prefix).
pub fn looks_binary(sample: &[u8]) -> bool {
  sample.contains(&0)
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
    assert_eq!(classify(Path::new("image.png")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("song.mp3")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("sheet.xlsx")), FileKind::Unsupported);
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
}
