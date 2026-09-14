# Video

Video files open read-only on a dedicated player page: a picture
(`gtk::Video` driven by a `gtk::MediaFile`) plus a play/pause button, a
seek slider with elapsed/total time, a volume slider and a file info line
(format, resolution when probed, duration, size). Playback uses
`gtk::MediaFile` (GStreamer-backed, no new Rust crate); files without an
installed decoder stay on the player page and show an honest hint with the
stream error instead of crashing. Video files are never editable and never
show the unsupported page.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed files still land on the player. Audio sniffing wins
over video sniffing for shared containers (Ogg, ASF) since their headers
cannot name the stream type cheaply; the `.ogv` and `.wmv` extensions still
route to video first.

| Kind | Extensions | Behavior |
|---|---|---|
| `Video` | `mp4`, `m4v`, `mkv`, `webm`, `mov`, `avi`, `ogv`, `flv`, `wmv`, `mpg`, `mpeg`, `3gp` | Read-only player, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_video(bytes: &[u8]) -> bool;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Video` when the file header carries known video magic (ISO
`ftyp` video brands, EBML, `RIFF....AVI `, Theora in Ogg, `FLV` version 1,
MPEG pack or sequence start), so e.g. MP4 bytes in a `.bin` file still
open in the player. ASF bytes are excluded on purpose (they also match WMA
audio), as are the `avif`/`heic` image and `M4A ` audio ISO brands, so
still images and audio never sniff as video.

Missing files are reported when the track is probed, not during
classification, so the player can show the reason hint.

## Supported Formats

Playback availability as checked against the target system libraries
(Arch Linux container, GTK 4.22, GStreamer 1.28 with only base/bad-libs
installed: container demuxers exist but no video decoders).

| Format | Decoder | Notes |
|---|---|---|
| `mp4`, `m4v` | GStreamer (`qtdemux` + H.264/AAC decoders) | Needs `gst-plugins-good` plus `gst-plugins-ugly` or `gst-libav` |
| `mkv` | GStreamer (`matroskademux` + video decoder) | Needs `gst-plugins-good` plus a codec package |
| `webm` | GStreamer (`matroskademux` + VP8/VP9/AV1 decoder) | Needs `gst-plugins-good` |
| `mov` | GStreamer (`qtdemux` + video decoder) | Needs `gst-plugins-good` plus a codec package |
| `avi` | GStreamer (`avidemux` + video decoder) | Needs `gst-plugins-good` plus a codec package |
| `ogv` | GStreamer (`oggdemux` + Theora decoder) | Needs `gst-plugins-base` plus `gst-plugins-base` Theora |
| `flv` | GStreamer (`flvdemux` + video decoder) | Needs `gst-plugins-good` plus a codec package |
| `wmv` | GStreamer (`asfdemux` + WMV decoder) | Needs `gst-plugins-ugly` or `gst-libav` |
| `mpg`, `mpeg` | GStreamer (`mpegpsdemux` + MPEG decoder) | Needs `gst-plugins-good` or `gst-libav` |
| `3gp` | GStreamer (`qtdemux` + H.264/AAC decoders) | Needs `gst-plugins-good` plus `gst-plugins-ugly` or `gst-libav` |

```rust
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", /* ... */ "mpeg", "3gp"];
```

> **Note:** Without the matching GStreamer plugin the file still opens on
> the player page, but pressing play shows `video.load_failed` with the
> stream error. Add the plugin packages to the ISO package list instead of
> changing code (see `## Dependencies`).

## VideoMeta

`src/model.rs` probes file size, format label and a best-effort header
duration plus resolution without decoding, so opening files stays cheap
and works even when no decoder is installed.

### probe_video

```rust
pub fn probe_video(path: &Path) -> Result<VideoMeta, String>;
```

Probes a video file and returns its metadata.

- Returns `Ok` with format label, file size and the header duration plus
  resolution when an ISO base media parser applies (both are `None`
  otherwise; the player then shows the runtime stream duration once
  prepared).
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing or empty, or the file cannot be read.

```rust
let meta = model::probe_video(path)?;
println!("{} {:?} {:?}", meta.format, meta.duration_secs, meta.resolution);
```

### VideoMeta fields

| Field | Type | Description |
|---|---|---|
| `format` | `String` | Short label (`MP4`, `MKV`, ...; `Video` when unknown) |
| `duration_secs` | `Option<f64>` | Header duration in seconds (`None` when unparsable) |
| `resolution` | `Option<(u32, u32)>` | Header resolution in pixels (`None` when unparsable) |
| `file_bytes` | `u64` | File size in bytes |

```rust
pub fn has_duration(&self) -> bool;
pub fn has_resolution(&self) -> bool;
```

`has_duration` returns true when a positive finite header duration is
known. `has_resolution` returns true when a non-zero header resolution is
known.

### Header parsers

Pure functions over byte slices, unit tested with synthetic headers:

```rust
pub fn m4a_duration(bytes: &[u8]) -> Option<f64>;
pub fn mp4_resolution(bytes: &[u8]) -> Option<(u32, u32)>;
pub fn video_format_label(ext: &str) -> &'static str;
```

- `m4a_duration` reads `moov/mvhd` timescale and duration (v0 and v1) and
  is shared with audio, since MP4 audio and video share the container.
- `mp4_resolution` reads width and height from the first `moov/trak/tkhd`
  box (16.16 fixed point, v0 and v1 layouts).
- Every parser returns `None` for truncated or inconsistent headers; the
  player shows `--:--` for the unknown side of the time label and omits
  the resolution from the info line.

## Time and Seek Helpers

The video player reuses the audio helpers in `src/model.rs` (unit tested,
see [Audio.md](Audio.md)):

```rust
pub fn format_audio_time(total_secs: f64) -> String;
pub fn format_audio_time_opt(total_secs: Option<f64>) -> String;
pub fn clamp_audio_seek(position: f64, duration: f64) -> f64;
```

- `format_audio_time` renders `m:ss` or `h:mm:ss` past one hour.
- `format_audio_time_opt` renders `--:--` when the duration is unknown.
- `clamp_audio_seek` clamps a position into `0..=duration`.

## Player UI

`src/views/preview.rs` adds a `video` page to the content stack with a
picture, a title, an info line and the controls (SF Pro Display). The
controls are only visible for video; `Edit` and `Save` are hidden and
`Ctrl+S` is a no-op for video.

| Control | Key | Behavior |
|---|---|---|
| Picture | — | `gtk::Video` driven by the `MediaFile`, never autoplays |
| `Play` / `Pause` | `video.play` / `video.pause` | Toggles the `MediaStream`, label follows the live state |
| Seek slider | `video.position` | 0..1000 permille of the known duration, seeks when released |
| `elapsed / total` | `video.position` | `%elapsed% / %total%`, unknown total shows `--:--` |
| Volume slider | `video.volume` | 0..100 percent, applied to the stream immediately |

The info line on the page shows `%format%, %resolution%%duration%, %size%,
read-only` (`status.video`), e.g. `MP4, 640 x 480, 1:05, 4.1 MB,
read-only` (resolution is empty when unknown). There is no bottom status
line. The window follows the live system color scheme (Dark
`#1d1d1d`, Light `#ececec`) via `app.auto_color_scheme()`; no background
is hardcoded.

Playback never autoplays: opening a file prepares the stream paused and
the user presses play. A 250 ms timer tick refreshes the slider, the time
label, the play/pause label and the info line from the stream (the stream
duration wins over the header duration once the pipeline reports one).
Switching files stops the previous stream, drops it with its tick and
detaches the picture; closing the window stops playback as well
(close-request plus unrealize), so no video (and no audio bed) survives
the file or the window.

## Error Cases

Broken video files stay on the `video` page and show
`video.load_failed` with the reason; they never fall through to the
unsupported page and never crash.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Empty file | `file is empty` |
| Corrupt file | Stream error from `MediaStream::error()` |
| Missing decoder | Stream error naming the missing plugin path |
| Audio-only file | `video.audio_only` hint (stream has no video track) |

> **Note:** Header probing reads at most a 64 KiB prefix; full files are
> never loaded into memory. Resolution is only probed for ISO base media
> (`mp4`, `m4v`, `mov`, `3gp`); other containers report the live stream
> duration instead.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%elapsed%`, `%total%`, `%format%`,
`%resolution%`, `%duration%`, `%size%`, `%reason%`). Only `en_us` and
`de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `video.play` | `Play` | `Abspielen` |
| `video.pause` | `Pause` | `Pause` |
| `video.volume` | `Volume` | `Lautstärke` |
| `video.position` | `%elapsed% / %total%` | `%elapsed% / %total%` |
| `video.load_failed` | `Cannot play this video file: %reason%` | `Diese Videodatei kann nicht abgespielt werden: %reason%` |
| `video.audio_only` | `This file has audio but no video track` | `Diese Datei enthält Audio, aber keine Videospur` |
| `status.video` | `%format%, %resolution%%duration%, %size%, read-only` | `%format%, %resolution%%duration%, %size%, schreibgeschützt` |

```json
{
  "status.video": "%format%, %resolution%%duration%, %size%, read-only"
}
```

## Dependencies

Playback uses `gtk::Video` plus `gtk::MediaFile` from the existing `gtk4`
0.9 dependency (GStreamer-backed `GtkMediaStream`: `play`, `pause`,
`seek`, `duration`, `timestamp`, `volume`, `has_video` in microseconds
plus a 250 ms `glib` timer tick). Declared in `Cargo.toml`:

```toml
gtk = { package = "gtk4", version = "0.9" }
```

No existing dependency version was changed and no new Rust crate was
added. The new system requirement is the GStreamer codec set for the ISO
package list: `gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`,
`gst-plugins-ugly` and optionally `gst-libav` (covers mp4/m4v/mov/3gp,
mkv, webm, avi, ogv, flv, wmv and mpg/mpeg).

## Usage / Example

Open a video file from the command line or the native file dialog:

```bash
cargo run -- /path/to/clip.mp4
```

Press `Play`, drag the seek slider to jump (`1:12 / 3:25`), and lower the
volume slider. There is no edit mode and no save path for video.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Video);
let meta = model::probe_video(path)?;
let label = model::format_audio_time_opt(meta.duration_secs);
let seek = model::clamp_audio_seek(90.0, meta.duration_secs.unwrap_or(0.0));
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Image.md](Image.md) – read-only picture viewer
- [Audio.md](Audio.md) – read-only audio player
