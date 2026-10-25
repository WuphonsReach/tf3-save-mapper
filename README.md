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

Each layer is a group marked as an Inkscape layer with a readable name (`inkscape:groupmode="layer"`, `inkscape:label`), so the Layers panel lists them in drawing order and each can be hidden or locked. As in the Python prototype, it also has `class="layer"` and its id in `data-layer`, so a page script can find the layers with `g.layer`. A group that holds one vehicle, depot or badge inside a layer is not a layer.

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
- `--mpp M` sets the metres per pixel: a multiple of 4 (the heightmap's spacing), or 1 or 2 on a crop. The default is 8 for the whole map, and 4 on a crop and with `--raster` or `--png`. A coarser picture cuts the terrain from every (M / 4)th point of the heightmap; a finer one only scales the picture, since the heightmap has no more detail. Line widths and label sizes that are set in pixels follow it.
- `--game DIR`: the game's install folder, the one holding `base/`. By default the usual Steam folders and their library folders are tried.
- `--mods DIR`: the mod.io game's mods folder, the one holding a folder per mod, read for a mod's depots (so far the Boathouse mod's boathouse). By default the mods folders beside the usual Steam folders are tried (for the snap, `~/snap/steam/common/mod.io/common/10640/mods`), then `~/.local/share/mod.io/common/10640/mods`. Without one a boathouse is still drawn, as a dot, but with no catchment and no coverage circle.
- `--contour M` sets the metres between height lines and bands (default 20); `--no-bands` leaves the colour bands out.
- `--raster` draws the terrain as one picture embedded in the SVG (a PNG in a data URI): the same height colours with hill shading lit from the north-west, and the water from light to dark blue with depth. Everything else is drawn on top as before. It is 4 m per pixel unless `--mpp` says otherwise, and the file is megabytes where the vector terrain's is hundreds of kilobytes.
- `--png` writes one flat PNG instead of the SVG: the `--raster` terrain with the roads and rails drawn into its pixels, each as wide as its lanes and at least one pixel, and nothing else (no stations, depots, industries, vehicles, labels, grid or rose; the options for those are refused). A `tf3map:view` text chunk holds a JSON record of where the picture lies: the map size, the metres per pixel, which way north is, the map box it shows (`x-min`, `x-max`, `y-min`, `y-max`) and the crop.
- Every SVG carries a `<metadata>` element with a `tf3map:view` record (the same names as the PNG's chunk, as attributes), so a picture can be turned back into map coordinates without its command line. The record names the build in `generator-version` and, from a git checkout, `generator-commit` (the same text `tf3map --version` ends in: `tf3map 0.7.0 (a30365b0c1)`, with `-dirty` for uncommitted changes), so a stored map can be tied to the build that drew it. A source archive without git gives the plain version and no `generator-commit`. The record also lists the settings that differ from the defaults (`contour-step`, `index-every`, `bands`, `grid`, `grid-step`, `grid-lines`, `rose`, `zoom`, `industries`, `depots`, `stations`, `terrain`, `vehicles`, `coverage` and so on), so a map says how it was made; a default run lists none. The save's file name and the time are left out.
- Exit status, for a script that runs many saves: 0 the map was written; 1 something else failed (the save cannot be opened, no game install, the output cannot be written); 2 the arguments are wrong (or the crop lies outside the map, or shows less than 8 m of it); 3 the save was refused because the reader finds it unreadable. A failure writes one line `tf3map: MESSAGE` to stderr and no picture. Each run reads the install and the mods folder itself; that costs about 0.03 s on a small save, so a batch of saves is one run per save.
- `--no-terrain`, `--no-industries`, `--no-depots`, `--no-stations` and `--no-rose` leave a layer out.
- `--rose WHERE` puts the compass rose in a corner (`lower-right`, the default, `lower-left`, `upper-left`, `upper-right`), round the map point `X,Y`, or nowhere (`off`); `--rose-size M` sets its diameter (default 250 m, or an eighth of the picture's shorter side when that is less; a size given is used as it is).
- `--north sun` turns the map so that up is +x and right is -y, the north the sun's path gives, instead of the game's own (`--north game`, the default: up is +y).
- `--min-width M` draws no road or track narrower than M metres, so tracks stay visible on a whole-map view.
- `--road-width M` and `--rail-width M` set the width of an edge whose lanes cannot be read (default 16 and 4).
- `--keep-dead` also draws roads the game deleted but left in the save.
- `--coverage` draws the area each depot and maintenance building covers: a dashed circle at the catchment radius of its script, round its origin, under the roads, one sublayer per kind. A ship depot's ring in the game is sometimes tens of metres off its origin; this draws it on the origin.
- `--vehicles` draws the vehicles, coloured by kind. `--name SLOT=TEXT` writes TEXT beside the vehicle whose `data-slot` is SLOT (repeatable); a default name such as `Road Vehicle 279` is shortened to `RV279`.
- `--label A>B[=TEXT]` writes TEXT (or `A>B`) along the edge between the nodes with hex ids A and B, in either order, and marks its ends (repeatable); `--edge-ids` adds the edge's own entity id where the save's strip table gives it. A label is never moved; one whose edge is not in the picture is reported.
- `--grid N` puts marks every N metres, the 1000 m ones major; by default there are major marks only, about a third of the shorter side apart (1000 m at least on a whole map); a step that would give more than a million crosses is refused. `--no-grid` leaves the grid out, `--grid-lines auto|none|dots|faint` chooses which lines are drawn besides the crosses and `--grid-dot-step M` spaces the dots.

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

Your mileage will vary with the machine, the disk and what the save holds. This is what I have seen locally, not a promise.

Measured on 2026-10-24 with `tf3map` 0.5.0 (release build) on 30 catalog saves, picked evenly by file size from 207 distinct ones (14 to 394 MB). One machine, one save at a time, default options. `tf3map` time is the median of three runs and memory the highest peak; the Python prototype's `terrain_map.py` ran once with its defaults, which match ours. All 60 runs worked.

| Save file size | Saves | `tf3map` time | `tf3map` peak memory |
|---|---|---|---|
| under 50 MB | 7 | 0.1 to 0.4 s | 75 to 360 MB |
| 50 to 100 MB | 11 | 0.3 to 0.8 s | 250 to 490 MB |
| 100 to 200 MB | 9 | 0.7 to 1.9 s | 430 MB to 1.5 GB |
| over 200 MB | 3 | 1.6 to 2.6 s | 1.0 to 1.7 GB |

- **Typical save:** under a second and under 500 MB. The largest one measured (394 MB file, 13,948 road edges) took 2.6 s and 1.7 GB. All 30 together took 22.7 s.
- **Against the prototype:** a median of 100 times faster (67 to 156 times) and about 3 times less memory (2.5 to 3.1 times). The prototype took 9 s to 273 s per save and peaked at 5.1 GB, so keep its parallel runs few. For `tf3map`, budget at most about 1.7 GB per job when running several at once.
- **What drives it:** time and memory follow the size of the save's data stream more than the number of edges, so two saves with equal edge counts can differ.
- **Format 605:** six saves written by the newer 40425 build (format 605), including the 20,480 by 40,960 m Giant map, drew without a failure in 1.0 to 1.4 s and 0.9 to 1.1 GB each. On two of them the prototype's counts of road and track edges, depots, industry badges and station pieces equalled ours; the geometry was not compared.

Sample of the 30 (mod.io ids):

| mod.io id | File | Edges (road / track) | `tf3map` | Python prototype | Faster by |
|---|---|---|---|---|---|
| 6432410 | 14 MB | 487 / 0 | 0.11 s, 75 MB | 9 s, 224 MB | 82× |
| 6430148 | 43 MB | 1,290 / 0 | 0.33 s, 245 MB | 31 s, 601 MB | 92× |
| 6430332 | 72 MB | 1,917 / 3,617 | 0.58 s, 361 MB | 77 s, 996 MB | 132× |
| 6105556 | 93 MB | 5,964 / 105 | 0.64 s, 386 MB | 56 s, 1.1 GB | 87× |
| 6421217 | 118 MB | 3,004 / 4,000 | 0.83 s, 515 MB | 75 s, 1.4 GB | 91× |
| 6442125 | 189 MB | 5,115 / 5,134 | 1.2 s, 786 MB | 166 s, 2.3 GB | 139× |
| 6428444 | 195 MB | 4,198 / 7,038 | 1.9 s, 1.5 GB | 273 s, 4.2 GB | 141× |
| 6425884 | 225 MB | 8,849 / 8,317 | 1.6 s, 1.0 GB | 111 s, 3.0 GB | 69× |
| 6418069 | 394 MB | 13,948 / 730 | 2.6 s, 1.7 GB | 198 s, 5.1 GB | 75× |

Only time, memory and whether the run worked were compared in this round; the drawn contents were compared element by element earlier.

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
