//! A hand-built stream with the structures the reader looks for, and nothing
//! from a real save: a header, a model table, the terrain, a street-edge
//! table, a decoy, model instances (an industry, two vehicles, a road
//! maintenance building and a mod's boathouse), two
//! road-surface strip entries and a free-id list.

#![allow(dead_code)]

pub struct EdgeSpec {
    pub a: u32,
    pub b: u32,
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    /// What the reader should give: the type's path without
    /// `.street_template`, and without `infrastructure/` for `street/` and
    /// `track/` types, which are written under it.
    pub template: &'static str,
    /// The transport-mode word of each lane.
    pub lanes: &'static [u32],
    /// The mod that brings the type, empty for the game's own.
    pub mod_name: &'static str,
    /// How many objects and decorations stand on the edge.
    pub objects: u32,
    pub decorations: u32,
}

impl Default for EdgeSpec {
    fn default() -> Self {
        EdgeSpec {
            a: 1,
            b: 2,
            p0: [0.0; 3],
            p1: [50.0, 0.0, 0.0],
            template: "street/town/town_old_small",
            lanes: ROAD_LANES,
            mod_name: "",
            objects: 0,
            decorations: 0,
        }
    }
}

/// Sidewalk, two car lanes, sidewalk.
pub const ROAD_LANES: &[u32] = &[0x03, 0x1c, 0x1c, 0x03];
/// Two track lanes with catenary: ELECTRIC_TRAIN and TRAIN bits among others.
pub const TRACK_LANES: &[u32] = &[0xc180, 0xc180];

pub const MAP_WIDTH: u32 = 4096;
pub const MAP_HEIGHT: u32 = 2048;
pub const SEED: &str = "synthetic seed";
pub const CLIMATE: &str = "::/climates/temperate/temperate.clima";
/// The free-id list holds a head, a run of 1500 ascending ids and a tail.
pub const FREE_RUN_START: u32 = 10_000;
pub const FREE_RUN_LEN: u32 = 1500;
pub const FREE_HEAD: [u32; 3] = [500_000, 400_000, 300_000];
/// Unsorted ids after the run, one past 1,000,000 as in long games.
pub const FREE_TAIL: [u32; 3] = [800, 2_000_000, 700];
/// Words before the list's count word. The first is a count word too and
/// reaches the same end, but the list's own is the nearer one.
pub const FREE_DECOY: [u32; 8] = [1514, 21, 22, 23, 24, 25, 26, 27];

/// The terrain: the map in 256 m tiles, height value 0 at -50 m, 0.05 m per
/// unit, the water at 0 m (1000 units). Heights rise and fall across the map,
/// with a pit under water round `PIT`.
pub const TILES: (u32, u32) = (MAP_WIDTH / 256, MAP_HEIGHT / 256);
pub const OFFSET_Z: f32 = -50.0;
pub const WATER_LEVEL: f32 = 0.0;
/// Grid column and row of the pit's centre, and its radius in grid points.
pub const PIT: (usize, usize, usize) = (300, 200, 20);
/// The model id of the one industry placed, its position and facing.
pub const FARM_ID: u32 = 120;
pub const FARM_AT: (f32, f32) = (-500.0, 300.0);
pub const FARM_FACING: (f32, f32) = (0.6, 0.8);
/// A bus near the first edges, and a wagon; model ids, positions and facings.
pub const BUS_ID: u32 = 121;
pub const BUS_AT: (f32, f32) = (-905.5, 452.25);
pub const BUS_FACING: (f32, f32) = (0.28, -0.96);
pub const WAGGON_ID: u32 = 122;
pub const WAGGON_AT: (f32, f32) = (-850.75, 401.5);
pub const WAGGON_FACING: (f32, f32) = (-0.8, 0.6);
/// A road maintenance building: model id, position and facing.
pub const MAINT_ID: u32 = 123;
pub const MAINT_MODEL: &str = "depots/road/road_maint_station/road_maint_station.mdl";
pub const MAINT_AT: (f32, f32) = (1200.5, -600.25);
pub const MAINT_FACING: (f32, f32) = (0.8, -0.6);
/// The Boathouse mod's depot: model id (a mod's, after the barn's), path,
/// position and facing.
pub const BOATHOUSE_ID: u32 = 901;
pub const BOATHOUSE_MODEL: &str = "assets/ingo_bootsschuppen/ingo_bootsschuppen.mdl";
pub const BOATHOUSE_AT: (f32, f32) = (-1450.5, -320.4);
pub const BOATHOUSE_FACING: (f32, f32) = (-0.28, 0.96);
/// A strip entry along the first edge (both sides, two rows each), and one
/// with a single side, which pairs with no edge.
pub const STRIP_ID: u32 = 0x7de1;
pub const STRIP_ROWS: [(u8, f32, f32); 4] = [
    (0, -990.0, 504.5),
    (1, -990.0, 495.5),
    (0, -905.25, 504.5),
    (1, -905.25, 495.5),
];
pub const LONE_STRIP_ID: u32 = 0x7de2;

/// The height value at grid column `c`, row `r` of the stitched grid.
pub fn height(c: usize, r: usize) -> u16 {
    let (pc, pr, radius) = PIT;
    if c.abs_diff(pc).pow(2) + r.abs_diff(pr).pow(2) <= radius * radius {
        return 200 + (c % 7) as u16;
    }
    (2000 + 15 * (c % 97) + 7 * (r % 53)) as u16
}

/// The game's trailing empty zstd frame.
pub const EMPTY_FRAME: [u8; 9] = [0x28, 0xb5, 0x2f, 0xfd, 0x20, 0x00, 0x01, 0x00, 0x00];

fn u32s(out: &mut Vec<u8>, values: &[u32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn string(out: &mut Vec<u8>, text: &str) {
    u32s(out, &[text.len() as u32]);
    out.extend_from_slice(text.as_bytes());
}

/// Everything before the street-edge table, in the order `docs/header.md` gives.
pub fn header(version: u32) -> Vec<u8> {
    let mut b = b"tf**".to_vec();
    u32s(&mut b, &[version, 1950, MAP_WIDTH, MAP_HEIGHT, 2_433_000]);
    b.extend_from_slice(&1_000_000i64.to_le_bytes()); // money
    u32s(&mut b, &[17]); // counter
    // info: one pair, key "company" -> table with one pair, "level" -> 3.0
    u32s(&mut b, &[1, 3]);
    string(&mut b, "company");
    u32s(&mut b, &[4]);
    b.push(1);
    u32s(&mut b, &[1, 3]);
    string(&mut b, "level");
    u32s(&mut b, &[2]);
    b.extend_from_slice(&3.0f64.to_le_bytes());
    // mods: one, five strings and flags
    u32s(&mut b, &[1]);
    for text in ["id", "source", "path", "a mod name", ""] {
        string(&mut b, text);
    }
    u32s(&mut b, &[0]);
    b.push(0); // flag before the preview
    u32s(&mut b, &[2, 1]); // a 2 x 1 preview
    string(&mut b, "\x01\x02\x03\x04\x05\x06");
    b.extend_from_slice(&[0; 6 * 8]); // 6 x i64
    b.extend_from_slice(&[0; 11 * 4]); // 11 x i32
    u32s(&mut b, &[1, 7, 8]); // one (u32, u32) pair
    u32s(&mut b, &[0, 0]);
    b.extend_from_slice(&[0; 9 * 8]);
    u32s(&mut b, &[0, 0]);
    u32s(&mut b, &[1]); // labels
    string(&mut b, "%d Point(s) for Company Value");
    u32s(&mut b, &[5]);
    b.extend_from_slice(&[0; 4 + 8 + 8 + 4]);
    u32s(&mut b, &[1]); // config mods
    string(&mut b, "id");
    u32s(&mut b, &[3]); // resources
    for (k, v) in [
        ("climate", CLIMATE),
        ("economy", "::/economy/all.eco"),
        ("nameList", "::/names/all.names"),
    ] {
        string(&mut b, k);
        string(&mut b, v);
    }
    u32s(&mut b, &[1]); // params: the empty key holds the settings
    string(&mut b, "");
    u32s(&mut b, &[1, 3]);
    string(&mut b, "isMapEditor");
    u32s(&mut b, &[1]);
    b.push(0);
    string(&mut b, "mission");
    string(&mut b, "kind");
    b.push(1);
    u32s(&mut b, &[604]);
    string(&mut b, SEED);
    b
}

/// One record of the street-edge table, in the layout of `docs/road-network.md`;
/// 284 bytes for a game street type with four lanes and nothing on it.
pub fn edge_record(e: &EdgeSpec) -> Vec<u8> {
    let mut b = Vec::new();
    u32s(&mut b, &[e.a, e.b]);
    let d = [e.p1[0] - e.p0[0], e.p1[1] - e.p0[1], e.p1[2] - e.p0[2]];
    for v in [e.p0, e.p1, d, d] {
        for c in v {
            b.extend_from_slice(&c.to_le_bytes());
        }
    }
    u32s(&mut b, &[0, u32::MAX]);
    u32s(&mut b, &[e.objects]);
    for i in 0..e.objects {
        u32s(&mut b, &[e.a * 10 + i, 0x5a]);
    }
    u32s(&mut b, &[e.decorations]);
    for i in 0..e.decorations {
        u32s(&mut b, &[i + 3]);
        b.push(1);
    }
    u32s(&mut b, &[e.lanes.len() as u32]);
    for (i, modes) in e.lanes.iter().enumerate() {
        let offset = (i as f32 - 1.5) * 3.25;
        for f in [13.89f32, 3.5 + i as f32 * 0.25, 0.375] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.push(u8::from(i % 2 == 0));
        u32s(&mut b, &[*modes]);
        b.extend_from_slice(&offset.to_le_bytes());
    }
    b.extend_from_slice(&[1, 0, 0, 0, 0x7f, 0, 0, 0, 2]); // not decoded
    string(&mut b, e.mod_name);
    let path = if e.template.starts_with("street/") || e.template.starts_with("track/") {
        format!("infrastructure/{}", e.template)
    } else {
        e.template.to_owned()
    };
    string(&mut b, &format!("{path}.street_template"));
    string(&mut b, "");
    string(&mut b, "infrastructure/street/town/town_old.street");
    b
}

/// The model table: 120 animals first, as the game's table sorts them, then
/// the farm's main model, a bus, a wagon, a road maintenance building and
/// two mods' models whose ids skip ahead.
pub fn model_table() -> Vec<u8> {
    let mut b = Vec::new();
    let mut paths: Vec<String> = (0..FARM_ID)
        .map(|i| format!("animal/beast_{i}.mdl"))
        .collect();
    paths.push("industries/farm/farm_sm.mdl".into());
    paths.push("vehicle/bus/town_bus/town_bus.mdl".into());
    paths.push("vehicle/waggon/box/box_waggon.mdl".into());
    paths.push(MAINT_MODEL.into());
    let mods = [
        ("some_mod", "assets/some_mod/barn.mdl", 900),
        ("boathouse_mod", BOATHOUSE_MODEL, BOATHOUSE_ID),
    ];
    u32s(&mut b, &[(paths.len() + mods.len()) as u32]);
    for (i, path) in paths.iter().enumerate() {
        string(&mut b, "");
        string(&mut b, path);
        u32s(&mut b, &[i as u32]);
    }
    for (source, path, id) in mods {
        string(&mut b, source);
        string(&mut b, path);
        u32s(&mut b, &[id]);
    }
    b
}

/// The terrain record, then the heightmap tile by tile.
pub fn terrain() -> Vec<u8> {
    let (tx, ty) = TILES;
    let mut b = Vec::new();
    u32s(&mut b, &[tx, ty, 5]);
    for f in [4.0f32, 4.0, 0.05] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    u32s(&mut b, &[3]);
    for f in [OFFSET_Z, WATER_LEVEL] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    u32s(&mut b, &[u32::MAX, 0x0e, 0, 0, 0, tx * ty]);
    for i in 0..(tx * ty) as usize {
        let (row, col) = (i / tx as usize, i % tx as usize);
        u32s(&mut b, &[65 * 65]);
        for r in 0..65 {
            for c in 0..65 {
                b.extend_from_slice(&height(64 * col + c, 64 * row + r).to_le_bytes());
            }
        }
    }
    b
}

/// One model instance: the id and a 4 x 4 transform turned to `facing`.
pub fn instance(id: u32, at: (f32, f32), facing: (f32, f32)) -> Vec<u8> {
    let mut b = id.to_le_bytes().to_vec();
    let (fx, fy) = facing;
    for f in [
        fx, fy, 0.0, 0.0, -fy, fx, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, at.0, at.1, 61.25, 1.0,
    ] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    b
}

/// One row of the strip table: `0x41`, the entry's id, the side, four
/// bytes not read, the u32 4 twice and the side's x and y.
pub fn strip_row(id: u32, side: u8, x: f32, y: f32) -> Vec<u8> {
    let mut b = vec![0x41];
    b.extend_from_slice(&id.to_le_bytes());
    b.push(side);
    b.extend_from_slice(&[0x3c, 0, 0, 0]);
    u32s(&mut b, &[4, 4]);
    b.extend_from_slice(&x.to_le_bytes());
    b.extend_from_slice(&y.to_le_bytes());
    b
}

/// The whole stream: header, model table, terrain, a run of counters, the
/// edge table, a decoy record far from it, the farm's, vehicles',
/// maintenance building's and boathouse's instances, the strip entries and a free-id list near the end.
pub fn stream(edges: &[EdgeSpec]) -> Vec<u8> {
    let mut s = header(604);
    s.extend_from_slice(&model_table());
    s.extend_from_slice(&terrain());
    // The table's first record follows small counters, not a `.street`; the last is 0.
    for i in 0..40u32 {
        u32s(&mut s, &[if i == 39 { 0 } else { i + 1 }]);
    }
    for e in edges {
        s.extend_from_slice(&edge_record(e));
    }
    // A decoy: a `.street` string and bytes that read as a record, 2 MiB from the table.
    s.resize(s.len() + (2 << 20), 0);
    s.extend_from_slice(b".street");
    s.extend_from_slice(&edge_record(&EdgeSpec {
        a: 9001,
        b: 9002,
        p0: [0.0, 0.0, 0.0],
        p1: [50.0, 0.0, 0.0],
        template: "street/decoy/decoy",
        ..EdgeSpec::default()
    }));
    s.extend_from_slice(&[0x11; 13]);
    s.extend_from_slice(&instance(FARM_ID, FARM_AT, FARM_FACING));
    s.extend_from_slice(&[0x22; 9]);
    s.extend_from_slice(&instance(BUS_ID, BUS_AT, BUS_FACING));
    s.extend_from_slice(&instance(WAGGON_ID, WAGGON_AT, WAGGON_FACING));
    s.extend_from_slice(&[0x33; 5]);
    s.extend_from_slice(&instance(MAINT_ID, MAINT_AT, MAINT_FACING));
    s.extend_from_slice(&instance(BOATHOUSE_ID, BOATHOUSE_AT, BOATHOUSE_FACING));
    for (side, x, y) in STRIP_ROWS {
        s.extend_from_slice(&strip_row(STRIP_ID, side, x, y));
        s.extend_from_slice(&[0x13; 6]);
    }
    s.extend_from_slice(&strip_row(LONE_STRIP_ID, 0, -700.5, 470.0));
    // The free-id list: a count word, a head that breaks the ascending run,
    // the long run and some unsorted ids; then its trailer (2, a u32, a str
    // of digits) and words no id can be.
    while !s.len().is_multiple_of(4) {
        s.push(0);
    }
    u32s(&mut s, &FREE_DECOY);
    let count = FREE_HEAD.len() as u32 + FREE_RUN_LEN + FREE_TAIL.len() as u32;
    u32s(&mut s, &[count]);
    u32s(&mut s, &FREE_HEAD);
    let run: Vec<u32> = (FREE_RUN_START..FREE_RUN_START + FREE_RUN_LEN).collect();
    u32s(&mut s, &run);
    u32s(&mut s, &FREE_TAIL);
    u32s(&mut s, &[2, 1]);
    string(&mut s, "1729");
    u32s(&mut s, &[u32::MAX; 4]);
    s
}

/// The stream as a save file: one zstd frame and the empty one.
pub fn sav(stream: &[u8]) -> Vec<u8> {
    let mut file = zstd::encode_all(stream, 3).unwrap();
    file.extend_from_slice(&EMPTY_FRAME);
    file
}
