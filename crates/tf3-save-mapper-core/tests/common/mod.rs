//! A hand-built stream with the structures the reader looks for, and nothing
//! from a real save: a header, a street-edge table, a decoy and a free-id list.

#![allow(dead_code)]

pub struct EdgeSpec {
    pub a: u32,
    pub b: u32,
    pub p0: [f32; 3],
    pub p1: [f32; 3],
    pub template: &'static str,
}

pub const MAP_WIDTH: u32 = 4096;
pub const MAP_HEIGHT: u32 = 2048;
pub const SEED: &str = "synthetic seed";
pub const CLIMATE: &str = "::/climates/temperate/temperate.clima";
/// The free-id list holds these ids: a run of 1500 ascending ids.
pub const FREE_RUN_START: u32 = 10_000;
pub const FREE_RUN_LEN: u32 = 1500;

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

/// One record of the street-edge table; 291 bytes for the templates used here.
pub fn edge_record(e: &EdgeSpec) -> Vec<u8> {
    let mut b = Vec::new();
    u32s(&mut b, &[e.a, e.b]);
    let d = [e.p1[0] - e.p0[0], e.p1[1] - e.p0[1], e.p1[2] - e.p0[2]];
    for v in [e.p0, e.p1, d, d] {
        for c in v {
            b.extend_from_slice(&c.to_le_bytes());
        }
    }
    b.extend_from_slice(&[0x11; 124]); // lane data
    string(
        &mut b,
        &format!("infrastructure/{}.street_template", e.template),
    );
    u32s(&mut b, &[0]);
    string(&mut b, "infrastructure/street/town/town_old.street");
    b
}

/// The whole stream: header, a run of counters, the table, a decoy record far
/// from it, and a free-id list near the end.
pub fn stream(edges: &[EdgeSpec]) -> Vec<u8> {
    let mut s = header(604);
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
    }));
    // The free-id list: a count word, a head that breaks the ascending run,
    // then the long run, then words no id can be.
    while !s.len().is_multiple_of(4) {
        s.push(0);
    }
    let head = [500_000u32, 400_000, 300_000];
    u32s(&mut s, &[0, 0, head.len() as u32 + FREE_RUN_LEN]);
    u32s(&mut s, &head);
    let run: Vec<u32> = (FREE_RUN_START..FREE_RUN_START + FREE_RUN_LEN).collect();
    u32s(&mut s, &run);
    u32s(&mut s, &[u32::MAX; 4]);
    s
}

/// The stream as a save file: one zstd frame and the empty one.
pub fn sav(stream: &[u8]) -> Vec<u8> {
    let mut file = zstd::encode_all(stream, 3).unwrap();
    file.extend_from_slice(&EMPTY_FRAME);
    file
}
