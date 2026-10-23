# tf3-save-mapper

Draw an SVG map of a Transport Fever 3 save: terrain, roads, rails, stations, towns. Unofficial, and not affiliated with or endorsed by Urban Games or Paradox Interactive.

The tool reads a `.sav` file and writes a vector map you can open in a browser or an editor. It is read-only: it never writes or changes a save.

## What the map shows

Today: roads and rails, drawn from the street edges in the save, on a plain background, as layers (`roads`, `rails`, one sublayer per street or track type) that Inkscape and similar editors show as layers. Curves follow the stored bends. The picture is the whole map, north up and east right, in map metres.

Planned: stations, towns and terrain as vector contours.

Drawing choices: labels are left out rather than moved, one outline width, no inner lines, plain dark anchors. The compass follows the game's own axes (x east, y north).

## Using it

```
tf3map SAVE.sav                  # writes SAVE.svg in the current folder
tf3map SAVE.sav -o map.svg
```

Options: `--road-width M` and `--rail-width M` (metres, default 16 and 4), `--keep-dead` to also draw roads the game deleted but left in the save. `tf3map --help` lists them. A save is read, never written, and the output may not be a `.sav`.

A 280 MB save takes about a second and 1.1 GB of memory, because the whole decompressed stream is held.

## Download

Archives for Windows (x86_64), macOS (Apple silicon and Intel) and Linux (x86_64, static) are on the [Releases page](https://github.com/WuphonsReach/tf3-save-mapper/releases). Each archive has the binary, the README and the licences, and carries a build provenance attestation:

```
gh attestation verify ARCHIVE --repo WuphonsReach/tf3-save-mapper
```

The binaries are not code-signed, so Windows and macOS may warn on first run.

## Building

Needs Rust 1.99 or newer and a C compiler (for the bundled zstd library).

```
cargo build --release
```

The binary is `target/release/tf3map`.

## How it relates to other projects

- The `.sav` layout comes from [tf3-save-format](https://github.com/WuphonsReach/tf3-save-format), a set of notes on the format. This tool has its own small reader and does not depend on any other save library.
- [tf3-save-editor](https://github.com/TBK/tf3-save-editor) reads and writes saves. It is a separate project by TBK.

## Privacy

Saves hold details of the player's machine and account, such as the company name (which defaults to the account display name). Do not post a map or a bug report that shows them without checking first. Please do not attach `.sav` files or previews of other people's maps to issues.

## Licence

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Status

Early. Roads and rails work, and match the Python prototype in tf3-save-format edge for edge on the saves compared. Header fields beyond the map size are read only on formats 599 to 604.
