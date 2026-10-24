# tf3-save-mapper

Draw an SVG map of a Transport Fever 3 save: terrain, roads, rails, stations, depots and industries. It can also write a flat PNG of the terrain and the network. Unofficial, and not affiliated with or endorsed by Urban Games or Paradox Interactive.

The tool reads a `.sav` file and writes a vector map you can open in a browser or an editor. It is read-only: it never writes or changes a save.

## What the map shows

The whole map, north up and east right, in map metres, as layers that Inkscape and similar editors show as layers:

- **Terrain**: height bands every 20 m in a plain colour ramp, the water (lakes, sea and rivers), and height lines with every fifth one heavier. All vector shapes, cut from the save's heightmap. With `--raster`, one shaded picture instead (below).
- **Roads and rails** from the street edges in the save, one sublayer per street or track type. Each is as wide as its lanes, sidewalks included (a small town street 16 m, a track 4 m), and curves follow the stored bends.
- **Stations**: the docks, platforms and buildings of water, rail and street stations, with a border round each bus, tram or cargo station and an anchor on each ship landing.
- **Depots and maintenance buildings** for road, tram, rail and ship, one grey or brown per carrier, and, when asked for, the area each one maintains.
- **Industries**: each industry's footprint, and a badge with its icon.
- **Vehicles**, when asked for: one shape per vehicle, pointing where it faces.
- A **grid** of map coordinates: marks at round values of x and y, labelled along the top and left edges.
- A **compass rose**, in the lower right corner unless asked elsewhere.

The shapes of stations, depots and industry footprints come from the game's own files, read from your install when the tool runs (see below). Without an install the map still has the terrain, the network and the industry badges.

Drawing choices: labels are left out rather than moved, one outline width (1.5 m), no inner lines, plain dark anchors. By default the compass follows the game's own axes (x east, y north); `--north sun` turns the map to the north the sun's path gives. The colours are our own, not the game's.

## Using it

```
tf3map SAVE.sav                  # writes SAVE.svg in the current folder
tf3map SAVE.sav -o map.svg
tf3map SAVE.sav --png            # writes SAVE.png: the shaded terrain, roads and rails only
```

Options (`tf3map --help` lists them all):

- `--crop X,Y,R` draws only the square of half-width R metres round the map point X,Y (x east, y north, 0,0 the map's centre), at 4 m per pixel instead of 8. The picture keeps the whole map's frame, so its `viewBox` starts at the crop's corner. `--zoom N` makes the picture N times larger on screen.
- `--mpp M` sets the metres per pixel: a multiple of 4 (the heightmap's spacing), or 1 or 2 on a crop. The default is 8 for the whole map and 4 on a crop. A coarser picture cuts the terrain from every (M / 4)th point of the heightmap; a finer one only scales the picture, since the heightmap has no more detail. Line widths and label sizes that are set in pixels follow it.
- `--game DIR`: the game's install folder, the one holding `base/`. By default the usual Steam folders and their library folders are tried.
- `--mods DIR`: the mod.io game's mods folder, the one holding a folder per mod, read for a mod's depots (so far the Boathouse mod's boathouse). By default the mods folders beside the usual Steam folders are tried (for the snap, `~/snap/steam/common/mod.io/common/10640/mods`), then `~/.local/share/mod.io/common/10640/mods`. Without one a boathouse is still drawn, as a dot, but with no catchment and no coverage circle.
- `--contour M` sets the metres between height lines and bands (default 20); `--no-bands` leaves the colour bands out.
- `--raster` draws the terrain as one picture embedded in the SVG (a PNG in a data URI): the same height colours with hill shading lit from the north-west, and the water from light to dark blue with depth. Everything else is drawn on top as before. It is 4 m per pixel unless `--mpp` says otherwise, and the file is megabytes where the vector terrain's is hundreds of kilobytes.
- `--png` writes one flat PNG instead of the SVG: the `--raster` terrain with the roads and rails drawn into its pixels, each as wide as its lanes and at least one pixel, and nothing else (no stations, depots, industries, vehicles, labels, grid or rose; the options for those are refused). A `tf3map:view` text chunk holds a JSON record of where the picture lies: the map size, the metres per pixel, which way north is, the map box it shows (`x-min`, `x-max`, `y-min`, `y-max`) and the crop.
- Every SVG carries a `<metadata>` element with a `tf3map:view` record (the same names as the PNG's chunk, as attributes), so a picture can be turned back into map coordinates without its command line. The save's file name and the time are left out, so the output stays the same from run to run. `tf3map --version` ends in the git commit of the build (`tf3map 0.4.0 (2dd0963a8d)`, with `-dirty` for uncommitted changes), so it changes with every drawing change; a source archive without git gives the plain version.
- `--no-terrain`, `--no-industries`, `--no-depots`, `--no-stations` and `--no-rose` leave a layer out.
- `--rose WHERE` puts the compass rose in a corner (`lower-right`, the default, `lower-left`, `upper-left`, `upper-right`), round the map point `X,Y`, or nowhere (`off`); `--rose-size M` sets its diameter (default 250 m, at most an eighth of the picture's shorter side).
- `--north sun` turns the map so that up is +x and right is -y, the north the sun's path gives, instead of the game's own (`--north game`, the default: up is +y).
- `--min-width M` draws no road or track narrower than M metres, so tracks stay visible on a whole-map view.
- `--road-width M` and `--rail-width M` set the width of an edge whose lanes cannot be read (default 16 and 4).
- `--keep-dead` also draws roads the game deleted but left in the save.
- `--coverage` draws the area each depot and maintenance building covers: a dashed circle at the catchment radius of its script, round its origin, under the roads, one sublayer per kind. A ship depot's ring in the game is sometimes tens of metres off its origin; this draws it on the origin.
- `--vehicles` draws the vehicles, coloured by kind. `--name SLOT=TEXT` writes TEXT beside the vehicle whose `data-slot` is SLOT (repeatable); a default name such as `Road Vehicle 279` is shortened to `RV279`.
- `--label A>B[=TEXT]` writes TEXT (or `A>B`) along the edge between the nodes with hex ids A and B, in either order, and marks its ends (repeatable); `--edge-ids` adds the edge's own entity id where the save's strip table gives it. A label is never moved; one whose edge is not in the picture is reported.
- `--grid N` puts marks every N metres, the 1000 m ones major; by default there are major marks only, about a third of the shorter side apart (1000 m at least on a whole map). `--no-grid` leaves the grid out, `--grid-lines auto|none|dots|faint` chooses which lines are drawn besides the crosses and `--grid-dot-step M` spaces the dots.

A save is read, never written, and the output may not be a `.sav`. A 390 MB save takes about three seconds and 1.7 GB of memory, because the whole decompressed stream is held.

### The game install

To draw stations, depots and industry footprints, the tool reads a few text files from the game's `base/content` archives at run time: the station model files, the depot scripts and the industry scripts. It takes only the sizes and outlines it needs, keeps nothing, and ships none of the game's files. It never opens the game's program files. In the same way it reads a mod's depot script from the mods folder, for its catchment radius, pool and transport modes, and nothing else there.

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

## Speed

Measured on 2026-10-24, release build, one machine, one save at a time. `tf3map` ran with its default layers (median of three runs); the Python prototype's `terrain_map.py` ran once with `--depots --stations`. Wall-clock time and peak memory. Saves are catalog saves, named by mod.io id.

| mod.io id | Edges (road / track) | `tf3map` | Python prototype | Faster by |
|---|---|---|---|---|
| 6397878 | 1,109 / 2,203 | 0.34 s, 222 MB | 44 s, 605 MB | 130× |
| 6418734 | 1,252 / 0 | 0.36 s, 212 MB | 43 s, 578 MB | 118× |
| 6422263 | 3,909 / 5,397 | 1.2 s, 654 MB | 79 s, 1.8 GB | 66× |
| 6425796 | 5,176 / 7,152 | 1.8 s, 844 MB | 118 s, 2.4 GB | 67× |
| 6431760 | 5,295 / 8,217 | 1.9 s, 944 MB | 127 s, 2.8 GB | 67× |
| 6417707 | 8,100 / 13,009 | 1.9 s, 986 MB | 142 s, 3.1 GB | 74× |
| 6425884 | 8,849 / 8,317 | 2.1 s, 1.0 GB | 142 s, 3.2 GB | 67× |
| 6419666 | 7,896 / 10,665 | 2.2 s, 1.2 GB | 241 s, 3.6 GB | 108× |
| 6432180 | 8,193 / 7,487 | 2.8 s, 1.5 GB | 310 s, 4.1 GB | 109× |

The busy saves (over 1,000 vehicles, 120 to 190 industries, rail, ships and aircraft) take 1 to 3 seconds. Time and memory grow with the size of the save's data stream, not only with the number of edges.

A further sample of 69 catalog saves (every third entry of a list of 208) ran without a failure: median 0.7 s and 370 MB, slowest 2.9 s and 1.6 GB.

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
