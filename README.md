# tf3-save-mapper

Draw an SVG map of a Transport Fever 3 save: terrain, roads, rails, stations, towns. Unofficial, and not affiliated with or endorsed by Urban Games or Paradox Interactive.

The tool reads a `.sav` file and writes a vector map you can open in a browser or an editor. It is read-only: it never writes or changes a save.

## What the map shows

Planned, in this order:

1. Roads, from the street edges in the save.
2. Rails, stations and towns.
3. Terrain as vector contours.

Drawing choices: labels are left out rather than moved, one outline width, no inner lines, plain dark anchors. The compass follows the game's own axes (x east, y north).

## Building

Needs a current stable Rust toolchain. There is no code yet, so there is nothing to build.

## How it relates to other projects

- The `.sav` layout comes from [tf3-save-format](https://github.com/WuphonsReach/tf3-save-format), a set of notes on the format. This tool has its own small reader and does not depend on any other save library.
- [tf3-save-editor](https://github.com/TBK/tf3-save-editor) reads and writes saves. It is a separate project by TBK.

## Privacy

Saves hold details of the player's machine and account, such as the company name (which defaults to the account display name). Do not post a map or a bug report that shows them without checking first. Please do not attach `.sav` files or previews of other people's maps to issues.

## Licence

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Status

Early. The repository holds the licence, ignore rules and project notes only; the Cargo workspace, CI and the first roads-only slice come next.
