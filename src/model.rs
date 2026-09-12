//! File model for Preview basis: text detection, loading and saving.
//!
//! Supported text extensions are opened as editable text. Markdown gets a
//! rendered preview mode plus a raw edit mode. Anything else is rejected as
//! unsupported with a hint (binary files are never loaded as text).

use std::path::{Path, PathBuf};

/// How Preview treats a file in the basis version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
  /// Plain editable text (`.txt`, `.py`, `.json`, ...).
  Text,
  /// Markdown: rendered preview plus raw edit mode.
  Markdown,
  /// Not supported yet (pdf, images, audio, video, office, binary).
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
  fn unsupported_classified() {
    assert_eq!(classify(Path::new("doc.pdf")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("song.mp3")), FileKind::Unsupported);
    assert_eq!(classify(Path::new("sheet.xlsx")), FileKind::Unsupported);
  }

  #[test]
  fn binary_sniff() {
    assert!(looks_binary(b"hello\x00world"));
    assert!(!looks_binary(b"hello world\n"));
  }
}
