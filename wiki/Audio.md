# Audio

Audio files open read-only on a dedicated player page: the controls offer
a play/pause button, a seek slider with elapsed/total time, a volume slider
and a file info line (format, duration, size). Playback uses
`gtk::MediaFile` (GStreamer-backed, no new Rust crate); files without an
installed decoder stay on the player page and show an honest hint with the
stream error instead of crashing. Audio files are never editable and never
show the unsupported page.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed files still land on the player. Generic MP4 video
brands (`isom`, `mp42`) are excluded from sniffing so videos never land on
the audio page.

| Kind | Extensions | Behavior |
|---|---|---|
| `Audio` | `mp3`, `wav`, `flac`, `ogg`, `oga`, `opus`, `m4a`, `aac`, `wma`, `aiff`, `aif` | Read-only player, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_audio(bytes: &[u8]) -> bool;
pub fn parse_mpeg_frame(bytes: &[u8]) -> Option<u32>;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Audio` when the file header carries known audio magic (ID3 tag,
validated MPEG frame sync, ADTS sync, `RIFF....WAVE`, `fLaC`, `OggS`,
`FORM....AIFF/AIFC`, `ftypM4A `, ASF header GUID), so e.g. WAV bytes in a
`.bin` file still open in the player. Frame-sync checks validate the
header bits (reserved MPEG version, layer and bitrate values reject), so
random bytes rarely match.

Missing files are reported when the track is probed, not during
classification, so the player can show the reason hint.

## Supported Formats

Playback availability as checked against the target system libraries
(Arch Linux container, GTK 4.22, GStreamer 1.28 with only base/bad-libs
installed: `playbin` exists but no audio decoders).

| Format | Decoder | Notes |
|---|---|---|
| `mp3` | GStreamer (`mpg123` plugin) | Needs `gst-plugins-ugly` on the system |
| `wav` | GStreamer (`wavparse`) | Needs `gst-plugins-good` on the system |
| `flac` | GStreamer (`flacdec`) | Needs `gst-plugins-good` on the system |
| `ogg`, `oga` | GStreamer (`vorbisdec`) | Needs `gst-plugins-base` on the system |
| `opus` | GStreamer (`opusdec`) | Needs `gst-plugins-base` on the system |
| `m4a`, `aac` | GStreamer (`faad`/`avdec_aac`) | Needs `gst-plugins-bad` or `gst-libav` |
| `wma` | GStreamer (`asfdemux` + WMA decoder) | Needs `gst-plugins-ugly`/`gst-libav` |
| `aiff`, `aif` | GStreamer (`aiffparse`) | Parser in base, codec in base |

```rust
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", /* ... */ "aiff", "aif"];
```

> **Note:** Without the matching GStreamer plugin the file still opens on
> the player page, but pressing play shows `audio.load_failed` with the
> stream error. Add the plugin packages to the ISO package list instead of
> changing code (see `## Dependencies`).

## AudioMeta

`src/model.rs` probes file size, format label and a best-effort header
duration without decoding, so opening files stays cheap and works even
when no decoder is installed.

### probe_audio

```rust
pub fn probe_audio(path: &Path) -> Result<AudioMeta, String>;
```

Probes an audio file and returns its metadata.

- Returns `Ok` with format label, file size and the header duration when
  a header parser applies (`duration_secs` is `None` otherwise; the player
  then shows the runtime stream duration once prepared).
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing or empty, or the file cannot be read.

```rust
let meta = model::probe_audio(path)?;
println!("{} {:?}", meta.format, meta.duration_secs);
```

### AudioMeta fields

| Field | Type | Description |
|---|---|---|
| `format` | `String` | Short label (`MP3`, `WAV`, ...; `Audio` when unknown) |
| `duration_secs` | `Option<f64>` | Header duration in seconds (`None` when unparsable) |
| `file_bytes` | `u64` | File size in bytes |

```rust
pub fn has_duration(&self) -> bool;
```

Returns true when a positive finite header duration is known.

### Header duration parsers

Pure functions over byte slices, unit tested with synthetic headers:

```rust
pub fn wav_duration(bytes: &[u8]) -> Option<f64>;
pub fn flac_duration(bytes: &[u8]) -> Option<f64>;
pub fn aiff_duration(bytes: &[u8]) -> Option<f64>;
pub fn m4a_duration(bytes: &[u8]) -> Option<f64>;
pub fn ogg_duration(prefix: &[u8], suffix: &[u8]) -> Option<f64>;
pub fn mp3_duration_estimate(prefix: &[u8], file_len: u64) -> Option<f64>;
pub fn wma_duration(bytes: &[u8]) -> Option<f64>;
pub fn audio_format_label(ext: &str) -> &'static str;
```

- `wav_duration` divides the first `data` chunk size by the `fmt` byte
  rate (declared sizes, so large files probe correctly from the prefix).
- `flac_duration` reads STREAMINFO total samples over sample rate.
- `aiff_duration` divides COMM sample frames by the 80-bit extended rate.
- `m4a_duration` reads `moov/mvhd` timescale and duration (v0 and v1).
- `ogg_duration` divides the last-page granule (minus Opus pre-skip) by
  the Vorbis rate from the ident header (Opus always runs at 48000 Hz).
- `mp3_duration_estimate` divides the file size by the first valid frame
  bitrate (skips ID3v2); VBR files only get an approximation, the player
  reports the exact runtime duration instead.
- `wma_duration` reads the ASF File Properties play duration
  (100-nanosecond units).
- Every parser returns `None` for truncated or inconsistent headers; the
  player shows `--:--` for the unknown side of the time label.

## Time and Seek Helpers

Pure helpers in `src/model.rs`, unit tested:

```rust
pub fn format_audio_time(total_secs: f64) -> String;
pub fn format_audio_time_opt(total_secs: Option<f64>) -> String;
pub fn clamp_audio_seek(position: f64, duration: f64) -> f64;
```

- `format_audio_time` renders `m:ss` (`0:05`, `2:05`) or `h:mm:ss` past
  one hour (`1:01:01`); non-finite and negative inputs show `0:00`.
- `format_audio_time_opt` renders `--:--` when the duration is unknown.
- `clamp_audio_seek` clamps a position into `0..=duration` and returns
  `0.0` when the duration is not positive or either value is non-finite.

## Player UI

`src/views/preview.rs` adds an `audio` page to the content stack with a
title, an info line and the controls (SF Pro Display). The controls are
only visible for audio; `Edit` and `Save` are hidden and `Ctrl+S` is a
no-op for audio.

| Control | Key | Behavior |
|---|---|---|
| `Play` / `Pause` | `audio.play` / `audio.pause` | Toggles the `MediaStream`, label follows the live state |
| Seek slider | `audio.position` | 0..1000 permille of the known duration, seeks when released |
| `elapsed / total` | `audio.position` | `%elapsed% / %total%`, unknown total shows `--:--` |
| Volume slider | `audio.volume` | 0..100 percent, applied to the stream immediately |

The status line shows `%format%, %duration%, %size%, read-only`
(`status.audio`), e.g. `MP3, 3:25, 4.1 MB, read-only`. The info line on
the page shows the same text. The window follows the live system color
scheme (Dark `#1d1d1d`, Light `#ececec`) via `app.auto_color_scheme()`;
no background is hardcoded.

Playback never autoplays: opening a file prepares the stream paused and
the user presses play. A 250 ms timer tick refreshes the slider, the time
label, the play/pause label and the info line from the stream (the stream
duration wins over the header duration once the pipeline reports one).
Switching files stops the previous stream and drops it with its tick;
closing the window stops playback as well (close-request plus unrealize),
so no audio survives the file or the window.

## Error Cases

Broken audio files stay on the `audio` page and show
`audio.load_failed` with the reason; they never fall through to the
unsupported page and never crash.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Empty file | `file is empty` |
| Corrupt file | Stream error from `MediaStream::error()` |
| Missing decoder | Stream error naming the missing plugin path |

> **Note:** Duration probing reads at most a 64 KiB header prefix plus an
> 8 KiB tail (Ogg last page); full files are never loaded into memory.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%elapsed%`, `%total%`, `%format%`, `%duration%`,
`%size%`, `%reason%`). Only `en_us` and `de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `audio.play` | `Play` | `Abspielen` |
| `audio.pause` | `Pause` | `Pause` |
| `audio.volume` | `Volume` | `Lautstärke` |
| `audio.position` | `%elapsed% / %total%` | `%elapsed% / %total%` |
| `audio.load_failed` | `Cannot play this audio file: %reason%` | `Diese Audiodatei kann nicht abgespielt werden: %reason%` |
| `status.audio` | `%format%, %duration%, %size%, read-only` | `%format%, %duration%, %size%, schreibgeschützt` |

```json
{
  "status.audio": "%format%, %duration%, %size%, read-only"
}
```

## Dependencies

Playback uses `gtk::MediaFile` from the existing `gtk4` 0.9 dependency
(GStreamer-backed `GtkMediaStream`: `play`, `pause`, `seek`, `duration`,
`timestamp`, `volume` in microseconds plus a 250 ms `glib` timer tick).
Declared in `Cargo.toml`:

```toml
gtk = { package = "gtk4", version = "0.9" }
```

No existing dependency version was changed and no new Rust crate was
added. The new system requirement is the GStreamer codec set for the ISO
package list: `gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`,
`gst-plugins-ugly` and optionally `gst-libav` (covers mp3, wav, flac,
ogg/opus, aac/m4a, wma and aiff).

## Usage / Example

Open an audio file from the command line or the native file dialog:

```bash
cargo run -- /path/to/song.mp3
```

Press `Play`, drag the seek slider to jump (`1:12 / 3:25`), and lower the
volume slider. There is no edit mode and no save path for audio.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Audio);
let meta = model::probe_audio(path)?;
let label = model::format_audio_time_opt(meta.duration_secs);
let seek = model::clamp_audio_seek(90.0, meta.duration_secs.unwrap_or(0.0));
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Image.md](Image.md) – read-only picture viewer
