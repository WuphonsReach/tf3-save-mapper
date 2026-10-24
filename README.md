# tf3-save-mapper

Draw an SVG map of a Transport Fever 3 save: terrain, roads, rails, stations, depots and industries. Unofficial, and not affiliated with or endorsed by Urban Games or Paradox Interactive.

The tool reads a `.sav` file and writes a vector map you can open in a browser or an editor. It is read-only: it never writes or changes a save.

## What the map shows

The whole map, north up and east right, in map metres, as layers that Inkscape and similar editors show as layers:

- **Terrain**: height bands every 20 m in a plain colour ramp, the water (lakes, sea and rivers), and height lines with every fifth one heavier. All vector shapes, cut from the save's heightmap.
- **Roads and rails** from the street edges in the save, one sublayer per street or track type. Each is as wide as its lanes, sidewalks included (a small town street 16 m, a track 4 m), and curves follow the stored bends.
- **Stations**: the docks, platforms and buildings of water, rail and street stations, with a border round each bus, tram or cargo station and an anchor on each ship landing.
- **Depots and maintenance buildings** for road, tram, rail and ship, one grey or brown per carrier.
- **Industries**: each industry's footprint, and a badge with its icon.
- A **compass rose** in the lower right corner.

The shapes of stations, depots and industry footprints come from the game's own files, read from your install when the tool runs (see below). Without an install the map still has the terrain, the network and the industry badges.

Drawing choices: labels are left out rather than moved, one outline width (1.5 m), no inner lines, plain dark anchors. The compass follows the game's own axes (x east, y north). The colours are our own, not the game's.

## Using it

```
tf3map SAVE.sav                  # writes SAVE.svg in the current folder
tf3map SAVE.sav -o map.svg
```

Options (`tf3map --help` lists them all):

- `--game DIR`: the game's install folder, the one holding `base/`. By default the usual Steam folders and their library folders are tried.
- `--contour M` sets the metres between height lines and bands (default 20); `--no-bands` leaves the colour bands out.
- `--no-terrain`, `--no-industries`, `--no-depots`, `--no-stations` and `--no-rose` leave a layer out.
- `--min-width M` draws no road or track narrower than M metres, so tracks stay visible on a whole-map view.
- `--road-width M` and `--rail-width M` set the width of an edge whose lanes cannot be read (default 16 and 4).
- `--keep-dead` also draws roads the game deleted but left in the save.

A save is read, never written, and the output may not be a `.sav`. A 390 MB save takes about three seconds and 1.7 GB of memory, because the whole decompressed stream is held.

### The game install

To draw stations, depots and industry footprints, the tool reads a few text files from the game's `base/content` archives at run time: the station model files, the depot scripts and the industry scripts. It takes only the sizes and outlines it needs, keeps nothing, and ships none of the game's files. It never opens the game's program files.

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
- The industry and anchor icons are our own drawings from tf3-save-format's `tools/icons/`, under the same licence as this tool.

## Privacy

Saves hold details of the player's machine and account, such as the company name (which defaults to the account display name). Do not post a map or a bug report that shows them without checking first. Please do not attach `.sav` files or previews of other people's maps to issues.

## Licence

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Status

Early. The layers match the Python prototype in tf3-save-format element for element on the saves compared, apart from where we chose otherwise (widths from the save's own lanes, one outline width). Header fields beyond the map size are read only on formats 599 to 604. Older saves without a model table get no stations, depots or industries.
