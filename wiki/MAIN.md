# Preview – Wiki

Preview is the TontooOS document viewer basis: a 960x640 TontooUI window
with an empty state (centered open button), a text viewer with line
numbers and a rendered Markdown preview plus raw edit mode. Saving is
manual only (`Save` button, `Ctrl+S`, close dialog with Cancel / Save /
Don't Save). Files open via CLI (`preview /path/to/file`) or the native
file dialog. It follows the live system color scheme and loads `en_us` /
`de_de` strings from `lang/`.

- Repository: https://github.com/TontooOS/TontooOS
- License: TCL v26.1
- Version: 26.1.0

## Feature Index

| Feature | File | Description |
|---|---|---|
| Main index | [MAIN.md](MAIN.md) | This page |
| Rules | [RULE.md](RULE.md) | Development and usage rules |
| Preview | [Preview.md](Preview.md) | Text viewer, Markdown preview/edit, manual save and localization |

## Quick Start

Run the Preview window from the repository root:

```bash
cargo run
```

Open a file directly:

```bash
cargo run -- /path/to/file.md
```

The window follows the system theme live (Dark `#1d1d1d`, Light
`#ececec`) and picks German strings when `LANG` starts with `de`.

See [Preview.md](Preview.md) for details.

## Changelog

- 2026-09-12: Initial Preview basis (empty state, text viewer with line
  numbers, Markdown preview/edit, manual save, CLI plus dialog open,
  `lang/en_us.json` and `lang/de_de.json`).
