//! File model for Preview: text detection, loading and saving plus PDF metadata.
//!
//! Supported text extensions are opened as editable text. Markdown gets a
//! rendered preview mode plus a raw edit mode. PDFs open read-only with
//! per-page text extraction (lazy: only the current page is extracted).
//! Images open read-only in a picture viewer (`gtk::Picture`): raster
//! formats are decoded with the pure-Rust `image` crate, SVG files are
//! handed to GTK (librsvg). Audio files open read-only in a compact player
//! (`gtk::MediaFile` backed by GStreamer): play/pause, seek, volume plus
//! file info (format, duration, size). Anything else is rejected as
//! unsupported with a hint (binary files are never loaded as text).

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
  /// Audio: read-only player (play/pause, seek, volume, file info).
  Audio,
  /// Not supported yet (video, office, binary).
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

/// Audio files open in the read-only player (see `wiki/Audio.md`).
/// `aac` covers raw ADTS streams, `m4a` covers MP4 audio, `oga` is the
/// Ogg audio extension alongside `ogg`.
pub const AUDIO_EXTENSIONS: &[&str] = &[
  "mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aiff", "aif",
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
  if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
    return FileKind::Audio;
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
/// carries known image magic, the file is treated as an image; when the
/// header carries known audio magic, the file is treated as audio. Other
/// kinds are never changed by sniffing.
pub fn classify_file(path: &Path) -> FileKind {
  let by_ext = classify(path);
  if by_ext == FileKind::Image || by_ext == FileKind::Audio {
    return by_ext;
  }
  let Ok(bytes) = read_prefix(path, 4096) else {
    return by_ext;
  };
  if sniff_image(&bytes) {
    return FileKind::Image;
  }
  if sniff_audio(&bytes) {
    return FileKind::Audio;
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

/// Returns true when the bytes carry a known audio magic number or tag
/// (see `wiki/Audio.md` for the format table). Frame-sync checks validate
/// the header bits so random bytes rarely match.
pub fn sniff_audio(bytes: &[u8]) -> bool {
  is_id3_magic(bytes)
    || parse_mpeg_frame(bytes).is_some()
    || is_adts_magic(bytes)
    || is_wav_magic(bytes)
    || is_flac_magic(bytes)
    || is_ogg_magic(bytes)
    || is_aiff_magic(bytes)
    || is_m4a_brand(bytes)
    || is_asf_magic(bytes)
}

fn is_id3_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(b"ID3")
}

/// Parse an MPEG audio frame header at the start of `bytes` and return
/// the bitrate in bits per second. Validates sync, MPEG version, layer,
/// bitrate index and sample-rate index (reserved values reject).
pub fn parse_mpeg_frame(bytes: &[u8]) -> Option<u32> {
  if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] & 0xE0 != 0xE0 {
    return None;
  }
  let version = (bytes[1] >> 3) & 0x03;
  let layer = (bytes[1] >> 1) & 0x03;
  let bitrate_index = (bytes[2] >> 4) & 0x0F;
  let rate_index = (bytes[2] >> 2) & 0x03;
  if version == 1 || layer == 0 || bitrate_index == 0 || bitrate_index == 15 || rate_index == 3 {
    return None;
  }
  let table: [u32; 15] = match (version, layer) {
    (3, 3) => [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448],
    (_, 3) => [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256],
    (3, 2) => [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384],
    (_, 2) | (3, 1) => [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320],
    (_, _) => [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
  };
  Some(table[bitrate_index as usize] * 1000)
}

/// ADTS (raw AAC) frame sync: `0xFFF` with layer bits `00`. Checked after
/// the MPEG frame test so MP3 frames (layer bits set) never match here.
fn is_adts_magic(bytes: &[u8]) -> bool {
  bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xF6 == 0xF0
}

fn is_wav_magic(bytes: &[u8]) -> bool {
  bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

fn is_flac_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(b"fLaC")
}

fn is_ogg_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(b"OggS")
}

fn is_aiff_magic(bytes: &[u8]) -> bool {
  bytes.len() >= 12
    && &bytes[..4] == b"FORM"
    && (&bytes[8..12] == b"AIFF" || &bytes[8..12] == b"AIFC")
}

/// MP4 audio (`ftyp` with an `M4A` major brand). Generic `isom`/`mp42`
/// brands are excluded so video files never sniff as audio.
fn is_m4a_brand(bytes: &[u8]) -> bool {
  if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
    return false;
  }
  let brand = &bytes[8..12];
  brand == b"M4A " || brand == b"m4a "
}

/// ASF header object GUID (`30 26 B2 75 ...`) used by WMA files.
fn is_asf_magic(bytes: &[u8]) -> bool {
  bytes.starts_with(&[
    0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62,
    0xCE, 0x6C,
  ])
}

/// Short display label for an audio extension (lowercase, without dot).
/// Unknown extensions report `Audio` so sniffed files still get a label.
pub fn audio_format_label(ext: &str) -> &'static str {
  match ext {
    "mp3" => "MP3",
    "wav" => "WAV",
    "flac" => "FLAC",
    "ogg" | "oga" => "Ogg",
    "opus" => "Opus",
    "m4a" => "M4A",
    "aac" => "AAC",
    "wma" => "WMA",
    "aiff" | "aif" => "AIFF",
    _ => "Audio",
  }
}

/// Read-only audio metadata probed without decoding: stat plus duration
/// parsed from the file header where cheap. Decoding is left to the
/// GStreamer-backed player, which also reports the exact duration at
/// runtime when a decoder is installed.
#[derive(Clone, Debug)]
pub struct AudioMeta {
  /// Short format label (`MP3`, `WAV`, ...).
  pub format: String,
  /// Duration in seconds when the header parse succeeded.
  pub duration_secs: Option<f64>,
  /// File size in bytes.
  pub file_bytes: u64,
}

impl AudioMeta {
  /// True when a header duration is known.
  pub fn has_duration(&self) -> bool {
    self.duration_secs.is_some_and(|d| d.is_finite() && d > 0.0)
  }
}

/// Probe an audio file: stat plus a best-effort header duration. Returns
/// a human-readable reason for missing files, directories and empty
/// files. Files whose duration cannot be parsed still probe fine
/// (`duration_secs` is `None`); the player shows the runtime duration
/// once the stream is prepared instead.
pub fn probe_audio(path: &Path) -> Result<AudioMeta, String> {
  let meta = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
  if meta.is_dir() {
    return Err("path is a directory".to_string());
  }
  if meta.len() == 0 {
    return Err("file is empty".to_string());
  }
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.to_lowercase())
    .unwrap_or_default();
  let prefix = read_prefix(path, 65536).map_err(|e| format!("cannot read file: {e}"))?;
  let suffix = read_suffix(path, 8192).unwrap_or_default();
  let duration_secs = probe_audio_duration(&ext, &prefix, &suffix, meta.len());
  Ok(AudioMeta {
    format: audio_format_label(&ext).to_string(),
    duration_secs,
    file_bytes: meta.len(),
  })
}

/// Read up to `max` trailing bytes of a file (used for the Ogg last-page
/// granule position). Returns fewer bytes for small files.
fn read_suffix(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
  use std::io::{Read, Seek, SeekFrom};
  let mut file = std::fs::File::open(path)?;
  let len = file.seek(SeekFrom::End(0))?;
  let take = (len.min(max as u64)) as usize;
  file.seek(SeekFrom::End(-(take as i64)))?;
  let mut buf = vec![0u8; take];
  file.read_exact(&mut buf)?;
  Ok(buf)
}

/// Duration for a known extension, falling back to magic-guarded parsers
/// so misnamed files still report a duration. Returns `None` when no
/// parser applies.
fn probe_audio_duration(ext: &str, prefix: &[u8], suffix: &[u8], file_len: u64) -> Option<f64> {
  let direct = match ext {
    "wav" => wav_duration(prefix),
    "flac" => flac_duration(prefix),
    "aiff" | "aif" => aiff_duration(prefix),
    "m4a" => m4a_duration(prefix),
    "ogg" | "oga" | "opus" => ogg_duration(prefix, suffix),
    "mp3" => mp3_duration_estimate(prefix, file_len),
    "wma" => wma_duration(prefix),
    "aac" => None,
    _ => None,
  };
  if direct.is_some() {
    return direct;
  }
  // Misnamed file: try every magic-guarded parser.
  if is_wav_magic(prefix) {
    wav_duration(prefix)
  } else if is_flac_magic(prefix) {
    flac_duration(prefix)
  } else if is_aiff_magic(prefix) {
    aiff_duration(prefix)
  } else if is_m4a_brand(prefix) {
    m4a_duration(prefix)
  } else if is_ogg_magic(prefix) {
    ogg_duration(prefix, suffix)
  } else if is_asf_magic(prefix) {
    wma_duration(prefix)
  } else if is_id3_magic(prefix) || parse_mpeg_frame(prefix).is_some() {
    mp3_duration_estimate(prefix, file_len)
  } else {
    None
  }
}

/// WAV duration from the RIFF chunks: first `data` chunk size divided by
/// the `fmt` byte rate. Returns `None` for truncated headers.
pub fn wav_duration(bytes: &[u8]) -> Option<f64> {
  if !is_wav_magic(bytes) {
    return None;
  }
  let mut byte_rate: Option<u32> = None;
  let mut offset = 12;
  while offset + 8 <= bytes.len() {
    let id = &bytes[offset..offset + 4];
    let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
    let data_start = offset + 8;
    let data_end = data_start.saturating_add(size).min(bytes.len());
    if id == b"fmt " && data_end >= data_start + 12 {
      byte_rate = Some(u32::from_le_bytes(
        bytes[data_start + 8..data_start + 12].try_into().ok()?,
      ));
    } else if id == b"data" {
      let rate = byte_rate? as f64;
      if rate <= 0.0 {
        return None;
      }
      // Declared chunk size, not the clamped prefix: large files probe
      // the same duration as small ones.
      return Some(size as f64 / rate);
    }
    offset = data_start + size + (size % 2);
    if offset <= data_start {
      return None;
    }
  }
  None
}

/// FLAC duration from the STREAMINFO block: total samples divided by the
/// sample rate. Returns `None` for truncated headers or a zero rate.
pub fn flac_duration(bytes: &[u8]) -> Option<f64> {
  if !is_flac_magic(bytes) || bytes.len() < 4 + 4 + 34 {
    return None;
  }
  // First metadata block must be STREAMINFO (type 0).
  if bytes[4] & 0x7F != 0 {
    return None;
  }
  let len = ((bytes[5] as usize) << 16) | ((bytes[6] as usize) << 8) | bytes[7] as usize;
  if len < 34 || bytes.len() < 8 + len {
    return None;
  }
  let info = &bytes[8..8 + 34];
  let rate = (((info[10] as u32) << 12) | ((info[11] as u32) << 4) | ((info[12] >> 4) as u32)) as f64;
  let total = (((info[13] & 0x0F) as u64) << 32)
    | ((info[14] as u64) << 24)
    | ((info[15] as u64) << 16)
    | ((info[16] as u64) << 8)
    | info[17] as u64;
  if rate <= 0.0 || total == 0 {
    return None;
  }
  Some(total as f64 / rate)
}

/// Convert an 80-bit extended-precision float (AIFF sample rate) to `f64`.
fn extended80_to_f64(bytes: &[u8]) -> Option<f64> {
  if bytes.len() < 10 {
    return None;
  }
  let negative = bytes[0] & 0x80 != 0;
  let exponent = ((((bytes[0] & 0x7F) as i32) << 8) | bytes[1] as i32) - 16383 - 63;
  let mantissa = u64::from_be_bytes(bytes[2..10].try_into().ok()?);
  if mantissa == 0 {
    return Some(0.0);
  }
  let value = mantissa as f64 * 2f64.powi(exponent);
  Some(if negative { -value } else { value })
}

/// AIFF duration from the COMM chunk: sample frames divided by the
/// 80-bit extended sample rate. Returns `None` for truncated headers.
pub fn aiff_duration(bytes: &[u8]) -> Option<f64> {
  if !is_aiff_magic(bytes) {
    return None;
  }
  let mut offset = 12;
  while offset + 8 <= bytes.len() {
    let id = &bytes[offset..offset + 4];
    let size = u32::from_be_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
    let data_start = offset + 8;
    let data_end = data_start.saturating_add(size).min(bytes.len());
    if id == b"COMM" && data_end >= data_start + 18 {
      let frames = u32::from_be_bytes(bytes[data_start + 2..data_start + 6].try_into().ok()?) as f64;
      let rate = extended80_to_f64(&bytes[data_start + 8..data_start + 18])?;
      if !rate.is_finite() || rate <= 0.0 {
        return None;
      }
      return Some(frames / rate);
    }
    offset = data_start + size + (size % 2);
    if offset <= data_start {
      return None;
    }
  }
  None
}

/// Find a direct child box `want` inside `data[start..end]`. Returns the
/// payload range `(body_start, body_end)`. Handles 32-bit sizes, `size ==
/// 1` (64-bit large size) and `size == 0` (box extends to `end`).
fn find_box(data: &[u8], start: usize, end: usize, want: &[u8; 4]) -> Option<(usize, usize)> {
  let mut offset = start;
  while offset + 8 <= end.min(data.len()) {
    let mut size = u32::from_be_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
    let id = &data[offset + 4..offset + 8];
    let mut header = 8;
    if size == 1 {
      if offset + 16 > end.min(data.len()) {
        return None;
      }
      size = u64::from_be_bytes(data[offset + 8..offset + 16].try_into().ok()?) as usize;
      header = 16;
    } else if size == 0 {
      size = end.saturating_sub(offset);
    }
    if size < header {
      return None;
    }
    let body_start = offset + header;
    let body_end = offset.saturating_add(size).min(end).min(data.len());
    if id == want {
      return Some((body_start, body_end));
    }
    if body_end <= offset {
      return None;
    }
    offset = body_end;
  }
  None
}

/// M4A duration from the `moov/mvhd` box: media duration divided by the
/// timescale (handles version 0 and version 1 headers).
pub fn m4a_duration(bytes: &[u8]) -> Option<f64> {
  let (moov_start, moov_end) = find_box(bytes, 0, bytes.len(), b"moov")?;
  let (mvhd_start, mvhd_end) = find_box(bytes, moov_start, moov_end, b"mvhd")?;
  let body = &bytes[mvhd_start..mvhd_end];
  if body.len() < 4 {
    return None;
  }
  let (timescale, duration) = if body[0] == 1 {
    if body.len() < 28 {
      return None;
    }
    (
      u32::from_be_bytes(body[20..24].try_into().ok()?) as f64,
      u64::from_be_bytes(body[24..32].try_into().ok()?) as f64,
    )
  } else {
    if body.len() < 20 {
      return None;
    }
    (
      u32::from_be_bytes(body[12..16].try_into().ok()?) as f64,
      u32::from_be_bytes(body[16..20].try_into().ok()?) as f64,
    )
  };
  if !timescale.is_finite() || timescale <= 0.0 || !duration.is_finite() {
    return None;
  }
  Some(duration / timescale)
}

/// Parse the first Ogg page: returns the sample rate plus Opus pre-skip.
/// Vorbis reports its own rate from the identification header; Opus audio
/// always runs at 48000 Hz output samples.
fn ogg_stream_info(prefix: &[u8]) -> Option<(f64, u32)> {
  if !is_ogg_magic(prefix) || prefix.len() < 27 + 1 {
    return None;
  }
  let segments = prefix[26] as usize;
  if prefix.len() < 27 + segments {
    return None;
  }
  // Collect the first packet (it may span several 255-byte segments).
  let mut packet: Vec<u8> = Vec::new();
  let mut offset = 27 + segments;
  for i in 0..segments {
    let seg_len = prefix[27 + i] as usize;
    let chunk = prefix.get(offset..offset + seg_len)?;
    packet.extend_from_slice(chunk);
    offset += seg_len;
    if seg_len < 255 {
      break;
    }
  }
  if packet.len() >= 30 && packet[0] == 0x01 && &packet[1..7] == b"vorbis" {
    let rate = u32::from_le_bytes(packet[12..16].try_into().ok()?) as f64;
    return if rate > 0.0 { Some((rate, 0)) } else { None };
  }
  if packet.len() >= 19 && &packet[..8] == b"OpusHead" {
    let preskip = u16::from_le_bytes(packet[10..12].try_into().ok()?) as u32;
    return Some((48000.0, preskip));
  }
  // The first packet decides the stream type; anything else is not audio.
  None
}

/// Granule position of the last Ogg page in `suffix` (total samples).
/// Pages with granule `-1` (no packet boundary) are skipped.
fn ogg_last_granule(suffix: &[u8]) -> Option<i64> {
  // Smallest page is the 27-byte header with zero segments.
  if suffix.len() < 27 {
    return None;
  }
  let mut pos = suffix.len().saturating_sub(4);
  loop {
    if suffix.len() >= pos + 4 && &suffix[pos..pos + 4] == b"OggS" {
      if suffix.len() >= pos + 14 {
        let granule = i64::from_le_bytes(suffix[pos + 6..pos + 14].try_into().ok()?);
        if granule >= 0 {
          return Some(granule);
        }
      }
    }
    if pos == 0 {
      return None;
    }
    pos -= 1;
  }
}

/// Ogg duration: last-page granule position (minus Opus pre-skip)
/// divided by the stream sample rate.
pub fn ogg_duration(prefix: &[u8], suffix: &[u8]) -> Option<f64> {
  let (rate, preskip) = ogg_stream_info(prefix)?;
  let granule = ogg_last_granule(suffix)? as f64 - preskip as f64;
  if !rate.is_finite() || rate <= 0.0 || granule <= 0.0 {
    return None;
  }
  Some(granule / rate)
}

/// Skip an ID3v2 tag at the start of `bytes` (10-byte header plus the
/// synchsafe size). Returns the offset of the first audio byte.
fn skip_id3v2(bytes: &[u8]) -> usize {
  if bytes.len() >= 10 && &bytes[..3] == b"ID3" {
    let size = ((bytes[6] as usize & 0x7F) << 21)
      | ((bytes[7] as usize & 0x7F) << 14)
      | ((bytes[8] as usize & 0x7F) << 7)
      | (bytes[9] as usize & 0x7F);
    10 + size
  } else {
    0
  }
}

/// MP3 duration estimate: file size divided by the first valid frame
/// bitrate. VBR files without a Xing header only get an approximation;
/// the player reports the exact runtime duration instead.
pub fn mp3_duration_estimate(prefix: &[u8], file_len: u64) -> Option<f64> {
  let start = skip_id3v2(prefix);
  let scan_end = (start + 8192).min(prefix.len());
  let mut offset = start;
  while offset + 4 <= scan_end {
    if let Some(bitrate) = parse_mpeg_frame(&prefix[offset..]) {
      if bitrate > 0 {
        return Some(file_len as f64 * 8.0 / bitrate as f64);
      }
    }
    offset += 1;
  }
  None
}

/// File Properties Object GUID of ASF (WMA) files.
const ASF_FILE_PROPERTIES: [u8; 16] = [
  0xA1, 0xDC, 0xAB, 0x8C, 0x47, 0xA9, 0xCF, 0x11, 0x8E, 0xE4, 0x00, 0xC0, 0x0C, 0x20,
  0x53, 0x65,
];

/// WMA duration from the ASF File Properties Object play duration
/// (100-nanosecond units). Returns `None` for truncated headers.
pub fn wma_duration(bytes: &[u8]) -> Option<f64> {
  if !is_asf_magic(bytes) || bytes.len() < 30 {
    return None;
  }
  let mut offset = 30; // header object GUID (16) + size (8) + object count (4) + reserved (2)
  while offset + 24 <= bytes.len() {
    let size = u64::from_le_bytes(bytes[offset + 16..offset + 24].try_into().ok()?) as usize;
    if size < 24 || offset + size > bytes.len() {
      // Object runs past the probed prefix; the duration is unknown.
      return None;
    }
    if bytes[offset..offset + 16] == ASF_FILE_PROPERTIES {
      if offset + 72 > bytes.len() {
        return None;
      }
      let units = u64::from_le_bytes(bytes[offset + 64..offset + 72].try_into().ok()?) as f64;
      return Some(units / 10_000_000.0);
    }
    if size == 0 {
      return None;
    }
    offset += size;
    if offset <= 30 {
      return None;
    }
  }
  None
}

/// Format seconds as `m:ss` (or `h:mm:ss` past one hour) for the player
/// time label. Non-finite and negative inputs show `0:00`.
pub fn format_audio_time(total_secs: f64) -> String {
  let total = if total_secs.is_finite() {
    total_secs.max(0.0).round() as u64
  } else {
    0
  };
  let (hours, mins, secs) = (total / 3600, (total % 3600) / 60, total % 60);
  if hours > 0 {
    format!("{hours}:{mins:02}:{secs:02}")
  } else {
    format!("{mins}:{secs:02}")
  }
}

/// Format an optional duration for the player: `--:--` when unknown.
pub fn format_audio_time_opt(total_secs: Option<f64>) -> String {
  total_secs.map(format_audio_time).unwrap_or_else(|| "--:--".to_string())
}

/// Clamp a seek position into `0..=duration`. Returns `0.0` when the
/// duration is not positive (nothing is seekable yet).
pub fn clamp_audio_seek(position: f64, duration: f64) -> f64 {
  if !duration.is_finite() || duration <= 0.0 {
    return 0.0;
  }
  if !position.is_finite() {
    return 0.0;
  }
  position.clamp(0.0, duration)
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
  fn audio_extensions_classified() {
    for name in [
      "song.mp3",
      "song.MP3",
      "tape.wav",
      "tape.WAV",
      "album.flac",
      "album.FLAC",
      "voice.ogg",
      "voice.oga",
      "call.opus",
      "call.OPUS",
      "track.m4a",
      "stream.aac",
      "legacy.wma",
      "sample.aiff",
      "sample.aif",
    ] {
      assert_eq!(classify(Path::new(name)), FileKind::Audio, "failed for {name}");
    }
  }

  #[test]
  fn audio_magic_sniffed() {
    assert!(sniff_audio(b"ID3\x04\x00\x00\x00\x00\x00\x00"));
    assert!(sniff_audio(&[0xFF, 0xFB, 0x90, 0x00])); // MPEG1 Layer 3 frame
    assert!(sniff_audio(&[0xFF, 0xF1, 0x50, 0x80])); // ADTS AAC frame
    assert!(sniff_audio(b"RIFF\x24\x00\x00\x00WAVE"));
    assert!(sniff_audio(b"fLaC\x10\x00\x00"));
    assert!(sniff_audio(b"OggS\x00\x02\x00\x00"));
    assert!(sniff_audio(b"FORM\x00\x00\x00\x00AIFF"));
    assert!(sniff_audio(b"FORM\x00\x00\x00\x00AIFC"));
    assert!(sniff_audio(b"\x00\x00\x00\x20ftypM4A \x00"));
    assert!(sniff_audio(&[
      0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62,
      0xCE, 0x6C,
    ]));
    assert!(!sniff_audio(b"hello world"));
    assert!(!sniff_audio(b"%PDF-1.4"));
    assert!(!sniff_audio(b"\x00\x00\x00\x20ftypisom\x00")); // video brand, not audio
    assert!(!sniff_audio(b""));
    // Reserved MPEG version (bit 3..4 == 01) is not a valid frame.
    assert!(!sniff_audio(&[0xFF, 0xEB, 0x90, 0x00]));
  }

  #[test]
  fn misnamed_audio_sniffed_as_audio() {
    let dir = std::env::temp_dir().join("preview-audio-test");
    let _ = std::fs::create_dir_all(&dir);
    // WAV magic without any extension still opens as audio.
    let no_ext = dir.join("misnamed-no-ext-audio");
    std::fs::write(&no_ext, minimal_wav(8000, 8)).expect("write wav magic");
    assert_eq!(classify_file(&no_ext), FileKind::Audio);
    // FLAC magic behind a `.bin` extension still opens as audio.
    let bin_ext = dir.join("misnamed.bin");
    std::fs::write(&bin_ext, minimal_flac(44100, 44100)).expect("write flac magic");
    assert_eq!(classify_file(&bin_ext), FileKind::Audio);
    let _ = std::fs::remove_file(&no_ext);
    let _ = std::fs::remove_file(&bin_ext);
  }

  #[test]
  fn unsupported_classified() {
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
  fn audio_time_formatting() {
    assert_eq!(format_audio_time(0.0), "0:00");
    assert_eq!(format_audio_time(5.0), "0:05");
    assert_eq!(format_audio_time(59.0), "0:59");
    assert_eq!(format_audio_time(60.0), "1:00");
    assert_eq!(format_audio_time(65.0), "1:05");
    assert_eq!(format_audio_time(3599.0), "59:59");
    assert_eq!(format_audio_time(3600.0), "1:00:00");
    assert_eq!(format_audio_time(3661.0), "1:01:01");
    assert_eq!(format_audio_time(7325.0), "2:02:05");
    assert_eq!(format_audio_time(-3.0), "0:00");
    assert_eq!(format_audio_time(f64::NAN), "0:00");
    assert_eq!(format_audio_time(f64::INFINITY), "0:00");
    assert_eq!(format_audio_time(61.6), "1:02");
  }

  #[test]
  fn audio_time_opt_formatting() {
    assert_eq!(format_audio_time_opt(None), "--:--");
    assert_eq!(format_audio_time_opt(Some(125.0)), "2:05");
  }

  #[test]
  fn audio_seek_clamping() {
    assert_eq!(clamp_audio_seek(5.0, 60.0), 5.0);
    assert_eq!(clamp_audio_seek(0.0, 60.0), 0.0);
    assert_eq!(clamp_audio_seek(60.0, 60.0), 60.0);
    assert_eq!(clamp_audio_seek(99.0, 60.0), 60.0);
    assert_eq!(clamp_audio_seek(-4.0, 60.0), 0.0);
    assert_eq!(clamp_audio_seek(5.0, 0.0), 0.0);
    assert_eq!(clamp_audio_seek(5.0, -2.0), 0.0);
    assert_eq!(clamp_audio_seek(f64::NAN, 60.0), 0.0);
    assert_eq!(clamp_audio_seek(5.0, f64::NAN), 0.0);
  }

  /// Build a minimal PCM WAV: 16-bit mono header plus `frames` of silence.
  fn minimal_wav(rate: u32, frames: u32) -> Vec<u8> {
    let data_len = frames * 2;
    let byte_rate = rate * 2;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend(std::iter::repeat(0u8).take(data_len as usize));
    out
  }

  /// Build a minimal FLAC header with a STREAMINFO block.
  fn minimal_flac(rate: u32, total: u64) -> Vec<u8> {
    let mut out = Vec::from(b"fLaC".as_slice());
    out.push(0x80); // last block + STREAMINFO type 0
    out.extend_from_slice(&[0, 0, 34]); // length 34
    out.extend_from_slice(&[16, 0, 16, 0, 0, 0, 0, 0, 0, 0]); // min/max block+frame
    out.push(((rate >> 12) & 0xFF) as u8);
    out.push(((rate >> 4) & 0xFF) as u8);
    // Rate low nibble plus stereo (channels - 1 = 1) and 16 bit (bps - 1 = 15).
    out.push((((rate & 0x0F) << 4) | 0x02) as u8);
    out.push(0xF0 | ((total >> 32) & 0x0F) as u8);
    out.push(((total >> 24) & 0xFF) as u8);
    out.push(((total >> 16) & 0xFF) as u8);
    out.push(((total >> 8) & 0xFF) as u8);
    out.push((total & 0xFF) as u8);
    out.extend_from_slice(&[0u8; 16]); // md5
    out
  }

  /// Encode an 80-bit extended float (AIFF sample rate) for tests.
  fn extended80(rate: f64) -> [u8; 10] {
    let mut out = [0u8; 10];
    if rate <= 0.0 {
      return out;
    }
    let exp = rate.log2().floor() as i32 + 16383;
    let mantissa = (rate / 2f64.powi(exp - 16383 - 63)).round() as u64;
    out[0] = ((exp >> 8) & 0x7F) as u8;
    out[1] = (exp & 0xFF) as u8;
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
  }

  /// Build a minimal AIFF header with a COMM chunk.
  fn minimal_aiff(frames: u32, rate: f64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"FORM");
    out.extend_from_slice(&46u32.to_be_bytes());
    out.extend_from_slice(b"AIFF");
    out.extend_from_slice(b"COMM");
    out.extend_from_slice(&18u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // channels
    out.extend_from_slice(&frames.to_be_bytes());
    out.extend_from_slice(&16u16.to_be_bytes()); // bits
    out.extend_from_slice(&extended80(rate));
    out
  }

  /// Build a minimal M4A header: `ftyp` plus `moov/mvhd` version 0.
  fn minimal_m4a(timescale: u32, duration: u32) -> Vec<u8> {
    let mut mvhd_body = Vec::new();
    mvhd_body.push(0); // version 0
    mvhd_body.extend_from_slice(&[0, 0, 0]); // flags
    mvhd_body.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // created + modified
    mvhd_body.extend_from_slice(&timescale.to_be_bytes());
    mvhd_body.extend_from_slice(&duration.to_be_bytes());
    let mut mvhd = Vec::new();
    mvhd.extend_from_slice(&((mvhd_body.len() + 8) as u32).to_be_bytes());
    mvhd.extend_from_slice(b"mvhd");
    mvhd.extend_from_slice(&mvhd_body);
    let mut moov = Vec::new();
    moov.extend_from_slice(&((mvhd.len() + 8) as u32).to_be_bytes());
    moov.extend_from_slice(b"moov");
    moov.extend_from_slice(&mvhd);
    let mut out = Vec::new();
    out.extend_from_slice(&24u32.to_be_bytes());
    out.extend_from_slice(b"ftyp");
    out.extend_from_slice(b"M4A ");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(b"M4A ");
    out.extend_from_slice(b"isom");
    out.extend_from_slice(&moov);
    out
  }

  /// Build a minimal Ogg Vorbis stream: first page with the ident header
  /// (rate) plus a final page with the total granule position.
  fn minimal_ogg_vorbis(rate: u32, total: u64) -> (Vec<u8>, Vec<u8>) {
    let mut ident = vec![0x01];
    ident.extend_from_slice(b"vorbis");
    ident.extend_from_slice(&[0, 0, 0, 0]); // version
    ident.push(2); // channels
    ident.extend_from_slice(&rate.to_le_bytes());
    ident.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // bitrates
    ident.extend_from_slice(&[1, 0]); // blocksize + framing
    let mut first = Vec::new();
    first.extend_from_slice(b"OggS");
    first.push(0); // version
    first.push(0x02); // beginning of stream
    first.extend_from_slice(&0u64.to_le_bytes()); // granule
    first.extend_from_slice(&1u32.to_le_bytes()); // serial
    first.extend_from_slice(&0u32.to_le_bytes()); // sequence
    first.extend_from_slice(&0u32.to_le_bytes()); // crc (unchecked by the parser)
    first.push(1); // one segment
    first.push(ident.len() as u8);
    first.extend_from_slice(&ident);
    let mut last = Vec::new();
    last.extend_from_slice(b"OggS");
    last.push(0);
    last.push(0x04); // end of stream
    last.extend_from_slice(&total.to_le_bytes());
    last.extend_from_slice(&1u32.to_le_bytes());
    last.extend_from_slice(&1u32.to_le_bytes());
    last.extend_from_slice(&0u32.to_le_bytes());
    last.push(0); // no segments
    (first, last)
  }

  /// Build a minimal ASF header with a File Properties Object duration.
  fn minimal_wma(play_units: u64) -> Vec<u8> {
    let mut out = vec![
      0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62,
      0xCE, 0x6C,
    ];
    out.extend_from_slice(&110u64.to_le_bytes()); // header object size
    out.extend_from_slice(&1u32.to_le_bytes()); // one object
    out.extend_from_slice(&[0, 0]); // reserved
    out.extend_from_slice(&ASF_FILE_PROPERTIES);
    out.extend_from_slice(&80u64.to_le_bytes()); // object size
    out.extend_from_slice(&[1u8; 16]); // file id
    out.extend_from_slice(&0u64.to_le_bytes()); // file size
    out.extend_from_slice(&0u64.to_le_bytes()); // creation date
    out.extend_from_slice(&1u64.to_le_bytes()); // packet count
    out.extend_from_slice(&play_units.to_le_bytes()); // play duration
    out.extend_from_slice(&play_units.to_le_bytes()); // send duration
    out
  }

  #[test]
  fn audio_header_durations() {
    // 1 second of 8 kHz 16-bit mono WAV.
    assert_eq!(wav_duration(&minimal_wav(8000, 8000)), Some(1.0));
    assert_eq!(wav_duration(b"RIFF\x00WAVE"), None);
    assert_eq!(wav_duration(b"hello world, not wav at all...."), None);
    // FLAC STREAMINFO: 44100 samples at 44100 Hz.
    let flac = minimal_flac(44100, 44100);
    assert_eq!(flac_duration(&flac), Some(1.0));
    assert_eq!(flac_duration(b"fLaC"), None);
    // AIFF COMM: 44100 frames at 44100 Hz.
    let aiff = minimal_aiff(44100, 44100.0);
    let duration = aiff_duration(&aiff).expect("aiff duration");
    assert!((duration - 1.0).abs() < 0.001, "got {duration}");
    assert_eq!(aiff_duration(b"FORM\x00\x00\x00\x00AIFF"), None);
    // M4A mvhd: 5000 units at 1000 units per second.
    assert_eq!(m4a_duration(&minimal_m4a(1000, 5000)), Some(5.0));
    assert_eq!(m4a_duration(b"too short"), None);
    // Ogg Vorbis: granule 88200 at 44100 Hz.
    let (first, last) = minimal_ogg_vorbis(44100, 88200);
    assert_eq!(ogg_duration(&first, &last), Some(2.0));
    assert_eq!(ogg_duration(&first, b"no pages here"), None);
    assert_eq!(ogg_duration(b"OggS", &last), None);
    // MP3 estimate: MPEG1 Layer 3 at 128 kbps over 16000 bytes = 1.0 s.
    let mut mp3 = vec![0xFF, 0xFB, 0x90, 0x00];
    mp3.extend(std::iter::repeat(0u8).take(15996));
    assert_eq!(mp3_duration_estimate(&mp3, 16000), Some(1.0));
    assert_eq!(mp3_duration_estimate(b"no frames here....", 16000), None);
    // WMA: 20 million 100-ns units = 2.0 s.
    assert_eq!(wma_duration(&minimal_wma(20_000_000)), Some(2.0));
    assert_eq!(wma_duration(b"too short for asf......."), None);
  }

  #[test]
  fn probe_audio_roundtrip() {
    let dir = std::env::temp_dir().join("preview-audio-test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("tone.wav");
    std::fs::write(&path, minimal_wav(8000, 16000)).expect("write wav");
    let meta = probe_audio(&path).expect("probe wav");
    assert_eq!(meta.format, "WAV");
    assert_eq!(meta.duration_secs, Some(2.0));
    assert!(meta.has_duration());
    assert!(meta.file_bytes > 44);
    // MP3 without a decoder still probes (duration is an estimate).
    let mp3_path = dir.join("song.mp3");
    let mut mp3 = vec![0xFF, 0xFB, 0x90, 0x00];
    mp3.extend(std::iter::repeat(0u8).take(15996));
    std::fs::write(&mp3_path, &mp3).expect("write mp3");
    let mp3_meta = probe_audio(&mp3_path).expect("probe mp3");
    assert_eq!(mp3_meta.format, "MP3");
    // Unknown header bytes probe with an unknown duration, never an error.
    let unknown = dir.join("mystery.ogg");
    std::fs::write(&unknown, b"OggS truncated").expect("write ogg stub");
    let unknown_meta = probe_audio(&unknown).expect("probe stub");
    assert_eq!(unknown_meta.format, "Ogg");
    assert_eq!(unknown_meta.duration_secs, None);
    assert!(!unknown_meta.has_duration());
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&mp3_path);
    let _ = std::fs::remove_file(&unknown);
  }

  #[test]
  fn probe_audio_reports_reasons() {
    assert!(probe_audio(Path::new("/nonexistent-preview-test/missing.mp3")).is_err());
    let dir = std::env::temp_dir().join("preview-audio-test");
    let _ = std::fs::create_dir_all(&dir);
    assert!(probe_audio(&dir).is_err());
    let empty = dir.join("empty.wav");
    std::fs::write(&empty, b"").expect("write empty");
    assert!(probe_audio(&empty).is_err());
    let _ = std::fs::remove_file(&empty);
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
