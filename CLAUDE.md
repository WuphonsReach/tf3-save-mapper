# tf3-save-mapper

Read-only command-line tool that draws an SVG map of a Transport Fever 3 `.sav`. Unofficial fan tool, licensed MIT OR Apache-2.0. Rust, as a Cargo workspace: `tf3-save-mapper-core` (container, header, edges, instances, terrain, SVG writer; no CLI code) and the binary crate (arguments, file output). The binary is `tf3map`, built from the `tf3-save-mapper` crate in `crates/tf3-save-mapper`; the core is `crates/tf3-save-mapper-core`. Both are `publish = false` until the user decides on crates.io.

Positioning: use a `tf3-` name; write "Transport Fever 3" only in the description and topics. The README carries an "unofficial, not affiliated with Urban Games" line.

## Workflow

- `main` is protected by a ruleset: no deletion, no force push, changes only through a pull request. Work on a branch; never commit to `main`.
- Do not push, open a PR or change repo visibility unless asked.
- Commits end with the Co-Authored-By line from the session's attribution reminder.
- Stay in this repo. Changes to `tf3-save-format` or `tf3-save-editor` only through a handoff the user requests.
- `gh` from snap prints nothing in this environment; run `/snap/gh/current/gh`.

## Rules for what goes in

- Never commit `.sav` files, decompressed streams, or previews or maps of other people's saves. `.gitignore` blocks the common forms (including `*.svg` outside `docs/images/`). Name files in `git add`; never `git add -A` or `git add .`.
- Never write or modify a `.sav`.
- Saves hold other people's local details. Company names default to the account display name; a mod's Lua error text can contain its full install path with the account name. Output, docs, tests and commit messages must mask them: `<name> Transport`, `<path>/mod.io/10640/mods/<id>/...`.
- Cite catalog saves by mod.io id only. Tests committed to the repo use a hand-built minimal synthetic stream. A golden test against real saves reads the path from an environment variable and skips when it is unset.
- Use the game's internal names (`cement`, `vegetables`, `ship`); the list is in `tf3-save-format/docs/glossary.md`.
- Offsets move between saves: find structures by pattern, never by fixed offset.
- Write the reader from the format notes in our own words. Never copy text or code from tf3-save-editor's `FORMAT.md`; credit it if used.

## Game install

- Never read, `strings`, dump or grep the game's native binaries: the `TransportFever3` executable, the bundled `.so` libraries, `model_editor/ModelEditor`.
- `base/content/` archives may be read for facts only, under the conditions in `tf3-save-format/CLAUDE.md` ("Game install"): no republishing string tables, no fonts, no music, no meshes or textures, and never bundle base data in the tool.

## Dependencies

Standard crates are fine if their licences are permissive. Tell the user about any heavy or unusual crate before adding it. Keep `rust-version` and edition in line with the installed toolchain.

## Reference

The Python prototype is in the `tools/` folder of the sibling `tf3-save-format` repo (`terrain_map.py`, `street_edges.py`, `terrain_vector.py`, `model_instances.py`, `save_header.py`, `tf3save.py`). Read `tools/README.md` and `docs/README.md` there first, and use `tools/facts_index.py --outline` rather than whole-note reads. Relevant notes: `docs/road-network.md`, `docs/compass.md` (x east, y north), `docs/entity-names.md`.

Drawing preferences: omit labels rather than move them; compass rose lower-right; one outline width (1.5 m); no inner lines; station colours brown, grey, light grey; plain dark anchors; vector terrain by default.
