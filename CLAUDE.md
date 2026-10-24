# tf3-save-mapper

Read-only command-line tool that draws an SVG map of a Transport Fever 3 `.sav`. Unofficial fan tool, licensed MIT OR Apache-2.0. Rust, as a Cargo workspace: `tf3-save-mapper-core` (container, header, edges, instances, terrain and contours, install reader, SVG writer with a module per layer; no CLI code) and the binary crate (arguments, file output). The binary is `tf3map`, built from the `tf3-save-mapper` crate in `crates/tf3-save-mapper`; the core is `crates/tf3-save-mapper-core`. Both are `publish = false` until the user decides on crates.io.

Positioning: use a `tf3-` name; write "Transport Fever 3" only in the description and topics. The README carries an "unofficial, not affiliated with Urban Games" line.

## Workflow

- `main` is protected by a ruleset: no deletion, no force push, changes only through a pull request. Work on a branch; never commit to `main`.
- Do not push, open a PR or change repo visibility unless asked.
- Commits end with the Co-Authored-By line from the session's attribution reminder.
- Stay in this repo. Changes to `tf3-save-format` or `tf3-save-editor` only through a handoff the user requests.
- `gh` from snap prints nothing in this environment; run `/snap/gh/current/gh`.

## Rules for what goes in

- Never commit `.sav` files, decompressed streams, or previews or maps of other people's saves. `.gitignore` blocks the common forms (including `*.svg` and `*.png` outside `docs/images/`). Name files in `git add`; never `git add -A` or `git add .`.
- Never write or modify a `.sav`.
- Saves hold other people's local details. Company names default to the account display name; a mod's Lua error text can contain its full install path with the account name. Output, docs, tests and commit messages must mask them: `<name> Transport`, `<path>/mod.io/10640/mods/<id>/...`.
- Cite catalog saves by mod.io id only. Tests committed to the repo use a hand-built minimal synthetic stream. A golden test against real saves reads the path from an environment variable and skips when it is unset.
- Use the game's internal names (`cement`, `vegetables`, `ship`); the list is in `tf3-save-format/docs/glossary.md`.
- Offsets move between saves: find structures by pattern, never by fixed offset.
- Write the reader from the format notes in our own words. Never copy text or code from tf3-save-editor's `FORMAT.md`; credit it if used.
- The icons in `crates/tf3-save-mapper-core/icons/` are copies of tf3-save-format's `tools/icons/` (our own drawings, same licence), built in with `include_str!`. Change them there and copy them over; a new industry key also needs a line in `ICONS` (`svg/industries.rs`). `.gitignore` lets `*.svg` in that folder through.

## Testing

- Committed tests use a hand-built stream (`crates/tf3-save-mapper-core/tests/common/mod.rs`): header, model table, terrain and heightmap, edge table, decoy, one industry instance, two vehicles, a road maintenance building, free-id list. Keep heightmap runs short (no long ascending run of u32 words near the stream's end, or the free-id search takes it). Keep its floats varied: with all-equal heights a misaligned read can pass the plausibility checks and make the table's first record ambiguous.
- `tests/golden.rs` runs on a real save when `TF3_MAPPER_TEST_SAVE` is set (and checks the edge count against `TF3_MAPPER_TEST_EDGES` if set) and does nothing otherwise. Never write that path into a committed file.
- To check a port against the Python prototype, compare per-edge: positions within 0.2 m and the curved/straight choice, after the free-id filter. Counts alone hide mismatches.
- For the other layers, run both tools on the same saves (`terrain_map.py --depots --stations`) and compare the SVGs element by element. The two share a frame (metres from the top-left corner) and the number formats, so key on `data-model`/`data-key` with `data-x`, `data-y`. Station pieces, depots, badges and footprints compare exactly; outline runs and border loops compare after normalising each loop's start; compare terrain paths by level. Differences that are ours by choice: edge widths from the save's lanes, and one 1.5 m outline width where the prototype scales outlines (and the landing clearance) by metres per pixel. Prototype bugs found so far are in `tf3-save-format/local/handoff-mapper-layers.md`; patch a scratchpad copy of `tools/` for them rather than porting them. The prototype takes about 90 s for a mid-size save, so run the saves in parallel in the background.
- For the grid, crop, zoom, rose, vehicle and label options, run both tools with the same options on the same saves and compare as above; shift the prototype's coordinates by the corner of our `viewBox` when cropped. Differences that are ours by choice: our picture is the exact box, where the prototype's is one pixel wider and taller (on a crop it keeps 1 to 2 extra edges and a vehicle or two from that strip, its grid lines are 8 m longer and its default rose sits a few metres off); our `--name` slot is `data-slot`, which counts every instance record, where the prototype counts vehicles only; `--vehicles` works without `--crop`; no `<metadata>` view record. Also written up in `tf3-save-format/local/handoff-mapper-options.md`.
- For `--raster` and `--mpp`, run the prototype with the same options and decode the terrain PNG from each SVG's data URI (the `terrain` layer's `<image>`); compare it pixel by pixel and its placement in map-frame metres, then the rest of the SVG as above. The pixels match exactly: keep the prototype's arithmetic (its `** 0.5`, its floor division when filling in the grid below 4 m per pixel, truncation to bytes). At an `--mpp` that does not divide the map (12, 20, ...) both terrain pictures stop short of the map's edge, but our `viewBox` stays the whole map where the prototype's shrinks, so the grid lines and the default rose differ by that strip.
- For `--png`, run `terrain_map.py --png` with the same options and compare the decoded pixels and the `tf3map:view` chunk. The pixels match except round the few edges whose lanes give another width than the prototype's template or default (16 m and 4 m); check that every differing pixel lies within a brush width of such an edge. The view record has the prototype's names and values except `source`, `generator-commit` and `generated-utc`, which we leave out (the output stays the same from run to run), and our `generator-version`.
- For `--coverage`, run the prototype with `--depots --coverage` and compare each circle keyed on `data-model`, `data-x`, `data-y` (centre, radius, ink, pool, modes, dash). Line and dash widths are pixels times metres per pixel in both. Differences that are ours by choice: a circle's group is `id="coverage-SLOT" class="coverage KIND"` where the prototype reuses the depot's `id="depot-SLOT" class="depot KIND"` (two elements with one id); on a crop the prototype also keeps every depot footprint within the radius, which lies outside the picture, where we keep footprints within 40 m and circles within their radius.
- Data quirks seen in real saves, not bugs: a few nodes (up to about 1.5 %) have two positions because leftover records reuse ids, and airport runways can reach past the map edge. An edge's lane widths can differ from today's template (`town_new_small` 14 m against 16 m in an older save); draw the save's. Numbers in the install's `.mdl` and script files can carry exponents (`2.28882e-07`); a reader that stops at `e` drops or shrinks pieces.

## Game install

- Never read, `strings`, dump or grep the game's native binaries: the `TransportFever3` executable, the bundled `.so` libraries, `model_editor/ModelEditor`.
- `base/content/` archives may be read for facts only, under the conditions in `tf3-save-format/CLAUDE.md` ("Game install"): no republishing string tables, no fonts, no music, no meshes or textures, and never bundle base data in the tool.
- `tf3map` reads the install at run time (`install.rs`: station `.mdl` boxes, depot and industry script ground faces) and keeps nothing. Tests never need an install.
- `.claude/hooks/tf3-install-guard.py` (copied from tf3-save-format) blocks the common ways of reading the binaries. It also fires on any command that mentions the install by name and pipes through `grep` without `-I`; put such text in a file or use the Edit tool.

## Dependencies

Standard crates are fine if their licences are permissive. Tell the user about any heavy or unusual crate before adding it. Keep `rust-version` and edition in line with the installed toolchain.

## Reference

The Python prototype is in the `tools/` folder of the sibling `tf3-save-format` repo (`terrain_map.py`, `street_edges.py`, `terrain_vector.py`, `model_instances.py`, `save_header.py`, `tf3save.py`). Read `tools/README.md` and `docs/README.md` there first, and use `tools/facts_index.py --outline` rather than whole-note reads. Relevant notes: `docs/road-network.md`, `docs/compass.md` (x east, y north), `docs/entity-names.md`.

Drawing preferences: omit labels rather than move them; compass rose lower-right; the grid is drawn by default (`--no-grid` leaves it out); one outline width (1.5 m); no inner lines; station colours brown, grey, light grey; plain dark anchors; vector terrain by default.
