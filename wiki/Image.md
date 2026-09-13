# Image

Image files open read-only on a dedicated picture page: the content
area shows the actual picture (`gtk::Picture`, SF Pro Display is used
for the surrounding UI text) with no in-page buttons. Zoom runs through
the header toolbar icons (`plus.magnifyingglass` /
`minus.magnifyingglass`); images open fitted into the viewport.
Raster formats are decoded with the pure-Rust
`image` crate (already in the workspace via `CoreIcon`); SVG files are
handed to GTK (librsvg). Images never show the unsupported page and are
never editable; broken files show an error hint with the reason instead.

## File Detection

`src/model.rs` classifies by extension first and falls back to magic-byte
sniffing, so misnamed files still land on the picture viewer. SVG markup
(`<svg` near the file start) is sniffed as well.

| Kind | Extensions | Behavior |
|---|---|---|
| `Image` | `png`, `jpg`, `jpeg`, `gif`, `bmp`, `webp`, `tiff`, `tif`, `svg`, `ico`, `avif`, `heif`, `heic` | Read-only picture viewer, no editing |

```rust
pub fn classify(path: &Path) -> FileKind;
pub fn classify_file(path: &Path) -> FileKind;
pub fn sniff_image(bytes: &[u8]) -> bool;
pub fn is_svg_file(path: &Path) -> bool;
```

`classify` uses the extension only. `classify_file` upgrades any other
result to `Image` when the file header carries known image magic, so
e.g. PNG bytes in a `.txt` file still render as a picture. Sniffing only
ever upgrades to `Image`; text and PDF results are never changed by it.

Missing files are reported when the picture is probed, not during
classification, so the viewer can show the reason hint.

## Supported Formats

Decoding availability as checked against the workspace (`image` 0.25
default formats) and the target system libraries:

| Format | Decoder | Notes |
|---|---|---|
| `png` | `image` crate | Full support |
| `jpg`, `jpeg` | `image` crate | Full support |
| `gif` | `image` crate | First frame only, no animation |
| `bmp` | `image` crate | Full support |
| `webp` | `image` crate | Full support |
| `tiff`, `tif` | `image` crate | Full support |
| `ico` | `image` crate | Full support |
| `avif` | `image` crate | Decoded by the bundled `ravif` support |
| `svg` | GTK via librsvg | Vector path, needs `librsvg` on the system |
| `heif`, `heic` | none bundled | Classified as image, shows a hint naming the missing decoder |

```rust
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", /* ... */ "heif", "heic"];
```

> **Note:** HEIF/HEIC files land on the image page with an
> `image.load_failed` hint because neither the `image` crate nor the
> system loaders decode them here. Bundling a decoder would need the
> `libheif` system library (see `## Dependencies`).

## ImageMeta

`src/model.rs` probes dimensions and file size without decoding full
pixels, so opening huge files stays cheap.

### probe_image

```rust
pub fn probe_image(path: &Path) -> Result<ImageMeta, String>;
```

Probes an image file and returns its metadata.

- Returns `Ok` with dimensions, file size and the downscaled display
  size.
- Returns `Err` with a human-readable reason when the path is a
  directory, the file is missing, or the content cannot be decoded
  (corrupt files never crash).

```rust
let meta = model::probe_image(path)?;
println!("{}x{}", meta.width, meta.height);
```

### ImageMeta fields

| Field | Type | Description |
|---|---|---|
| `width` | `u32` | Original pixel width (`0` when unknown) |
| `height` | `u32` | Original pixel height (`0` when unknown) |
| `file_bytes` | `u64` | File size in bytes |
| `downscaled` | `bool` | True when the display size was scaled down |
| `display_width` | `u32` | Pixels used for display |
| `display_height` | `u32` | Pixels used for display |
| `is_svg` | `bool` | True for SVG files (GTK render path) |

```rust
pub fn has_dimensions(&self) -> bool;
```

Returns true when real dimensions are known. SVG files without a
`width`/`height` or `viewBox` size report `0 x 0` and keep their natural
GTK size in the viewer.

### display_size

```rust
pub const MAX_IMAGE_DISPLAY: u32 = 2048;
pub fn display_size(width: u32, height: u32) -> (u32, u32);
```

Scales dimensions down so the longest side fits 2048 pixels, preserving
the aspect ratio. Smaller images keep their size. The status line shows
the original dimensions plus the `image.downscaled` note when scaling
applied.

### parse_svg_size

```rust
pub fn parse_svg_size(text: &str) -> Option<(u32, u32)>;
```

Reads the SVG canvas size in pixels: `width`/`height` attributes first
(an optional `px` suffix is accepted), then the `viewBox` size. Returns
`None` when no usable size is present.

## Zoom and Fit

Pure zoom math lives in `src/model.rs` so it is unit tested; the viewer
in `src/views/preview.rs` applies it through an explicit picture size.

### Functions

```rust
pub const MIN_IMAGE_ZOOM: f64 = 0.1;
pub const MAX_IMAGE_ZOOM: f64 = 8.0;
pub const IMAGE_ZOOM_STEP: f64 = 0.25;
pub fn clamp_image_zoom(zoom: f64) -> f64;
pub fn image_zoom_in(zoom: f64) -> f64;
pub fn image_zoom_out(zoom: f64) -> f64;
pub fn zoomed_size(base_width: u32, base_height: u32, zoom: f64) -> (u32, u32);
pub fn fit_zoom_for(base_w: u32, base_h: u32, view_w: i32, view_h: i32) -> f64;
pub fn format_file_size(bytes: u64) -> String;
```

- `image_zoom_in` / `image_zoom_out` move one 0.25 step, clamped to
  0.1–8.0.
- `zoomed_size` scales base pixels by the zoom factor (at least 1 pixel
  per side).
- `fit_zoom_for` computes the factor that fits base pixels into a
  viewport, preserving aspect; returns `1.0` when any size is zero.
- `format_file_size` renders `512 B`, `1.5 KB`, `5.0 MB` for the status
  line.

## Viewer UI

`src/views/preview.rs` adds an `image` page to the content stack with a
`gtk::Picture` and no in-page button bar. Zoom runs through the header
toolbar (`plus.magnifyingglass` / `minus.magnifyingglass`, sensitive
only for PDF and image pages); `Edit` and `Save` are hidden and `Ctrl+S`
is a no-op for images. Fit mode never sets an explicit picture size, so
`Contain` plus `can_shrink` keeps the picture inside the viewport on
every resize instead of overflowing into scrollable clipping; only
manual zoom sets an explicit size.

| Control | Key | Behavior |
|---|---|---|
| Header `plus.magnifyingglass` / `minus.magnifyingglass` | `pdf.zoom_in` / `pdf.zoom_out` (tooltips) | Factor in 0.25 steps, clamped to 0.1–8.0, leaves fit mode |

The status line shows `%width% x %height%, %size%, read-only`
(`status.image`), plus the `image.downscaled` note for huge images, or
`%size%, read-only` (`status.image_unknown`) when SVG dimensions are
unknown. The window follows the live system color scheme (Dark `#1d1d1d`,
Light `#ececec`) via `app.auto_color_scheme()`; no background is
hardcoded.

Fit is applied once when the image opens; manual zoom via the header
toolbar leaves fit mode.

## Error Cases

Broken images stay on the `image` page and show `image.load_failed`
with the reason; they never fall through to the unsupported page.

| Case | Reason hint |
|---|---|
| Missing file | `cannot stat file: ...` |
| Directory | `path is a directory` |
| Corrupt file | `cannot decode image: ...` |
| HEIF without decoder | `cannot decode image: ...` plus the missing-decoder note |
| SVG without size | Renders at natural GTK size, status shows file size only |

> **Note:** Decoding loads the full frame into memory; only the display
> pixbuf is downscaled. Extremely large files can use significant memory
> before the downscale applies.

## Localization

New keys in `lang/en_us.json` and `lang/de_de.json` (used with
`t_with` placeholders `%width%`, `%height%`, `%size%`, `%reason%`).
Only `en_us` and `de_de` exist.

| Key | `en_us` | `de_de` |
|---|---|---|
| `image.zoom_in` | `Zoom In` | `Vergrößern` |
| `image.zoom_out` | `Zoom Out` | `Verkleinern` |
| `image.fit` | `Fit Window` | `An Fenster anpassen` |
| `image.load_failed` | `Cannot open this image: %reason%` | `Dieses Bild kann nicht geöffnet werden: %reason%` |
| `image.downscaled` | `downscaled for display` | `für die Anzeige verkleinert` |
| `status.image` | `%width% x %height%, %size%, read-only` | `%width% x %height%, %size%, schreibgeschützt` |
| `status.image_unknown` | `%size%, read-only` | `%size%, schreibgeschützt` |

```json
{
  "status.image": "%width% x %height%, %size%, read-only"
}
```

## Dependencies

Raster decoding uses the `image` crate, which is already resolved in
the workspace (`CoreIcon` depends on `image 0.25`, same locked version
0.25.10, default formats include `avif`). Pixel upload uses
`gdk-pixbuf` 0.20 (already locked via `gtk4`, now also a direct
dependency). Declared in `Cargo.toml`:

```toml
image = "0.25"
gdk-pixbuf = "0.20"
```

No existing dependency version was changed. No new system dependency is
required for the bundled formats. SVG rendering needs the `librsvg`
system library on the target (ISO package list); HEIF rendering would
additionally need `libheif`, which is intentionally not bundled.

## Usage / Example

Open an image from the command line or the native file dialog:

```bash
cargo run -- /path/to/photo.jpg
```

Read the `800 x 600, 124.3 KB, read-only` status and adjust the size
with the header toolbar zoom icons. There is no edit mode and no save
path for images.

```rust
let kind = model::classify_file(path);
assert_eq!(kind, model::FileKind::Image);
let meta = model::probe_image(path)?;
let zoomed = model::zoomed_size(meta.display_width, meta.display_height, 2.0);
```

## Cross References

- [MAIN.md](MAIN.md) – wiki entry point and changelog
- [RULE.md](RULE.md) – wiki design system
- [Preview.md](Preview.md) – text viewer, Markdown preview/edit and manual save
- [Pdf.md](Pdf.md) – read-only PDF page viewer
- [Audio.md](Audio.md) – read-only audio player
