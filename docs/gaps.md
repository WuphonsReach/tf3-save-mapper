# Gaps: what the map does not draw yet

This lists what `tf3map` leaves out or gets wrong that we know of, so the next piece of work can be picked from one place. The Python prototype that came before it (`terrain_map.py` in tf3-save-format, removed in November 2026) never drew any of these either, so retiring it lost nothing.

Each entry says what is missing, the evidence, and what a fix needs. "Readable today" means the positions are in records the mapper already reads (model instances or street edges) and the shapes can come from the install, as they do for stations now. A note named here is in tf3-save-format's `docs/`. Saves are cited by mod.io id.

Where an entry says how many saves something is in, the count is from a model count over eight busy catalog saves: 6417707, 6419666, 6425796, 6431760, 6433459, 6435875, 6487589 and 6488211.

## In short

- **Never drawn, readable today:** airports, freight terminal platforms (rail and truck), landmarks and company headquarters, street station side buildings `_s1`. Aircraft are drawn, but as a car-sized grey shape.
- **Drawn wrong:** runways and taxiways are drawn as roads; false records at exactly 0,0 are drawn as industries and depots in two saves; road ends are round where the game's are flat; the grid's marks are too heavy on a small crop viewed larger.
- **Mod depots:** the buffer-stop depot and a Chinese EMU depot mod are not drawn.
- **Mod industries:** an industry built from other industries' pieces, with no main model of its own, has no marker or footprint.
- **Small:** the game date is not written into the SVG; the dead-edge filter checks only node ids; a removed industry's dead-end road still joined to the network is drawn.
- **Possible layers** the notes make room for: speed limits, electrified track, tram streets, parcel cells, lot tint, terrain changes between two saves, sunrise and sunset on the rose.
- **Blocked on the format:** towns, signals' positions, traffic lights, line routes, industry figures.

## Never drawn, readable today

- **Airports** The airfield, airport, heliport and helipad pieces (`stations/air/airfield/`, `airport/`, `heliport/`, `helipad/`: terminals, main buildings, hangars, runway pieces) are in 5 of the 8 saves. No station kind covers them. The airfield and airport hangars have a 550 m catchment and pool 16, the heliport 800 m and pool 4 (`depots.md`), so `--coverage` could draw them too. Needs: the boxes from the install's `stations/air` archive, a sublayer per kind and a colour; the hangar's catchment from its script.
- **Freight terminal platforms** The cargo platforms of rail and truck stations (`stations/rail/cargo_addons/rail_t_{bulk,flatbed,goods,liquids}_*` and `stations/road/cargo_addons/road_t_*`, each with `_start`, `_con`, `_end` and for some `_main` pieces) are in 6 and 7 of the 8 saves. Only the rail cargo station buildings (`rail/cargo/station_*`) are drawn, so a freight terminal shows as a building with no platforms. Needs: the pieces added to `STATION_KINDS`, joined like the water decks if the start, con and end pieces are parts of one platform.
- **Landmarks and company headquarters** `landmarks/*` (arena, Eiffel tower, lighthouse, Neuschwanstein, opera house, pyramid, ship museum, Statue of Liberty, Summer Palace, Westminster, White Heron Castle) and the headquarters modules `landmarks/hq/modules/*` are in all 8 saves. A landmark site's build progress is in the save but is not linked to its position (`landmarks.md`). Needs: footprints from the install, or a badge like the industries' if the boxes are too rough.
- **Street station side buildings `_s1`** `streetstation_sidebuilding_era_{a,b,c}_s1` is in 3 to 4 of the 8 saves; only the `_s2` building is drawn (`station-pieces.md` names both). Needs: the three `_s1` models in the street buildings kind, then a check that the box sits where the building does.
- **Aircraft and the hot air balloon** `vehicle/plane/`, `vehicle/helicopter/` and `vehicle/zeppelin/` (the balloon, a fun element: `fun-elements.md`) have no size or colour of their own, so they are drawn as the 6 x 2.5 m grey shape for anything unknown. Needs: a size and colour per class; the balloon could be left out or given a marker, since it is not a company vehicle.

## Drawn wrong

- **Runways and taxiways drawn as roads** Their edges are street edges with their own lane modes (`0xa00` and `0x800`, `road-network.md`) and no vehicle lanes (`street-templates.md`), and they fall into the roads layer. Needs: their own sublayer and style once the airport layer exists.
- **False records at exactly 0,0** Two of the 8 saves have records that fit the instance pattern but are no transform: their rows read `1 0 k 0` and `0 d d 0` with `d` a subnormal float (about 1e-40), so the translation comes out at 0, 0. They carry the ids of real models, so `tf3map` draws them on the origin: 165 industries and 8 rail maintenance buildings with their coverage circles in 6487589, 53 industries in 6488211. Needs: a rule that leaves out a record with a subnormal float in its first three rows (none was seen in a real transform; check over the catalogue saves first), or what structure they belong to, from the format side.
- **Road ends** A road that stops is drawn with a round cap (`stroke-linecap="round"` on every road sublayer); the game ends it flat (noted in tf3-save-format's shot-place skill). Needs: a flat cap where an end node has one edge, without opening gaps at junctions, where the round cap hides the seams.

- **Grid marks on a small crop** The grid's marks are sized in the picture's pixels at `--mpp` (a small cross's arm and every dot one pixel, a big cross's arm just short of the first dot), not to the size the picture is looked at. A small crop at the default 4 m per pixel is a small picture: `--crop 2830,3950,250` is 125 x 125 pixels with a 100 m grid. Shown larger, by a viewer or with `--zoom`, its marks grow with it: the big crosses' strokes come out about 6 m wide and the dots as heavy as the tracks, and the grid competes with what it should only locate. Seen on a 500 m crop shown 900 pixels across. Needs: the marks sized by `--zoom` as well as `--mpp` (a mark of so many pixels at the shown size), or a finer default `--mpp` on a small crop, and the labels' size checked with them.

## Mod depots

The roundhouse and the modern depot are drawn from their pieces' model boxes (`install::DEPOT_KINDS`). Their scripts work out their ground faces as they run, so the faces cannot be read the way the boathouse's radius is. Two kinds are still left out:

- **Buffer-stop depot** `grimes_prellbockdepot` (mod.io 5713333) sets pool 1 and a 1 m catchment (`depots.md`). Its one model is `content/rail/bumper_depot.mdl` in the mod's folder, with no ground face, so it would be a dot like the boathouse. The mod was enabled in 41 mined saves, but none of the 9 of those scanned had one built, so how a save names the model is unchecked. Needs: a save with one, to confirm the model path.
- **A road depot mod** `depots/road/road_depot/uep3_b04_crh_emu_*` (mod.io 6008076), a Chinese EMU depot built over the base road depot's folder, seen in one of the 8 saves (6487589). It is a train station with four depots: the script makes a station with three terminals and a 300 m catchment, plus four depot sub-constructions, one per gate. Its main model is `uep3_b04_crh_emu_depot_170m` or `_330m`, a box 170 or 324 m long. That save has one built (at about -607, -3434); its other four main-model instances were in the model preview block at the map's centre, which the reader leaves out. Needs: a kind with both main models as centre pieces, the station's catchment or the depots' for the coverage circle, and a screenshot to check the box.

## Mod industries

- **Industries with no main model** An industry is drawn from its main model (`industries/KEY/KEY_sm.mdl`, or the folder's `_particles.mdl`, `instances::industry_models`). A mod's industry can be built from the base game's industry pieces and its own assets with no such model: two military depots in a 1917 game, a weapons depot of saw mill buildings (`industries/saw_mill/msc_hangar.mdl`, `saw_mill_building.mdl`, `drying_kiln.mdl`, a brick works hangar) and a shell export depot of brick works, machine and tool factory pieces with `assets/military/shell_rack/` and `shell_depot_roads/`, both fenced with `assets/military/security_fence/`. The game shows both in its industries layer, and their entrance stubs are drawn, but the map has no marker or footprint for them. The loose-piece filter counts any `industries/` model, so their stubs stay. Their sidings are left out with every other industry's (`sidings::retain_without_sidings`), so with no footprint in their place the stub ends at nothing. Needs: a way to group an industry's pieces with no main model (by the mod's construction script from the mods folder, or by clustering the pieces round the entrance stub), then a footprint from the pieces' boxes.

## Small

- **The game date** The header's date (a Julian day number, `header.md`) is read but written neither into the SVG's `<desc>` nor into the `tf3map:view` record. Vehicles and animals are a moment in time, so a map should say when. Needs: the date in both, as a calendar date.
- **Edges whose own id was freed** `edges::retain_live` drops an edge when an end node is in the free-id list. In one test game a deleted track edge's own id was in that list while both its nodes stayed live (`signals-tests.md`, `town-leftovers.md`). Needs: also drop an edge whose entity id (from the strip table, as `--edge-ids` reads it) is free. Only some edges have a strip entry, so this catches only some.
- **Leftover tails joined to the network** What a removed industry leaves is dropped three ways: an end node in the free-id list (`edges::retain_live`), neither end node in the lane table (`lanes::retain_laned`), or a short group of industry road and track joined to nothing and over 200 m from every industry model (`loose::retain_near_industries`). A leftover piece still joined to a live road passes all three: the stub where a leftover run met the network, or a dead-end access road whose mine is gone (in a 1917 game of a 20480 x 40960 m map, one ends by a rail maintenance depot where a coal mine stood; not yet checked in the game). Needs: a rule for a dead-end run of small country road that ends where no industry stands, checked against screenshots, since a player's or a town's dead end looks the same.
- **Rail platform end pieces** `perron_5_end` and `perron_10_end` of all three eras (in 4 to 5 of the 8 saves) are not drawn. The edge pieces were left out on purpose for other kinds (`station-pieces.md`: as separate boxes their outlines ran inside every cell). Needs: a check against a screenshot of whether the platform's box already covers its end.
- **Ship depot rings** A ship depot's ring in the game is 0 to 66 m from its origin, in no fixed direction (`depots.md`); `--coverage` draws it on the origin. Needs: what the game centres it on.
- **Install and mods folder detection** Untested on Windows and macOS; the Flatpak mods folder is a guess. `--game` and `--mods` are the way round it.

## Possible layers

None of these is asked for yet; each has a reader in tf3-save-format's `tools/`.

- **Speed limits** One number per edge, lowered on bends; the notes give the game's colours (`road-speeds.md`; `edge_speeds.py`, `lane_paths.py`). Some track types hold no speed where it was read.
- **Electrified track** From the lane's flag byte (`0xc1`), not the `_catenary` in the type name, which stays after the wire is removed (`signals-tests.md`, `road-network.md`). The mapper does not keep lane modes today.
- **Tram streets** The `_tram` and `_tram_electrified` town streets and the tram-only streets (`vehicle-classes.md`, `street-templates.md`).
- **Parcel cells** 8 x 8 m cells with their built and overlap bits (`parcels.md`; `parcels.py --points`). Bit 0 is not only buildings.
- **Lot tint by zone and level** The ground-texture polygons carry the zone and level in their texture name (`a_res_l1`, `a_com_l1`; `editor-map-buildings.md`, `parcels.md`), which may get round the missing parcel ids. Most polygons are small door patches, not lots; no reader yet.
- **Terrain changes** The heightmap difference of two saves of one game (`terrain-edits.md`; `terrain_heights.py OLD NEW`).
- **Sunrise and sunset** Marks at -y and +y on the rose, as the notes' plots have them (`compass.md`; icons in `tools/icons/`).
- **Passenger catchment** 175 m round a stop group's centre (`transit-overlay.md`), beside the depots' `--coverage`. Placed only from the Transit Overlay mod's export so far.

## Blocked on the format

- **Towns:** no town position is stored; names and boundaries wait on it (`town-records.md`, `town-boundaries.md`).
- **Signals:** each is an entry in its edge's object list, but its place along the edge, side and direction are not found (`signals.md`).
- **Traffic lights:** in the junction table, which has no reader (`junctions.md`).
- **Line routes and stops:** nothing ties a line to edges or a vehicle to its line (`gaps/owners-and-links.md`).
- **Industry figures:** stock, closing date, a farm's fields: no rule links an industry's entity to its model instance (`industries.md`, `industry-lifecycle.md`, `farm-fields.md`).
- **Trees and forests:** placed and painted trees are records with positions, but which models are trees and how a generated map's forests are stored are open (`models.md`, `terrain.md`).
