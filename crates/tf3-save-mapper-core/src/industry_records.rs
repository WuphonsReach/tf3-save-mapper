//! Industries deleted in the map editor, and the older copies of an
//! industry's record that stack at one spot.
//!
//! A deleted industry keeps its construction record and every model
//! instance it had, its main model among them. An industry that lost a field
//! (to a road drawn across it, say) can leave an older record at the same spot,
//! often with a main instance of its own. Both would draw a marker the game
//! does not show.
//!
//! A construction record is an `industries/<path>.con` string with its own
//! length before it, a Lua table body, a 4 x 4 f32 transform, an i64 and seven
//! `vec<u32>` of entity ids. Found by the string, never by offset.
//!
//! - Deleted: the lists hold at least one id and every id is in the free-id
//!   list. Drop one main instance of the key within 1 m of the record.
//! - Older copy: some ids are free, not all, and a live record (no id free) of
//!   the same key within 1 m lists every id of it that is not free. Drop one
//!   main instance of the key within 1 m for each copy, but only while more
//!   than one is left at that spot.

use std::collections::{HashMap, HashSet};

use crate::instances::Instance;
use crate::reader::Reader;

/// Spots closer than this, metres, are the same spot.
const SAME_SPOT: f64 = 1.0;
/// A longer id list is not a record's.
const IDS_MAX: usize = 100_000;
const CON_START: &[u8] = b"industries/";

struct Construction {
    key: String,
    x: f64,
    y: f64,
    ids: Vec<u32>,
}

/// The end of `industries/<chars>.con` starting at `at`: the shortest run of
/// path characters, at least one, followed by `.con`.
fn con_path_end(data: &[u8], at: usize) -> Option<usize> {
    let ok = |b: u8| b.is_ascii_alphanumeric() || b"_./-".contains(&b);
    let mut p = at + CON_START.len();
    while p < data.len() && ok(data[p]) {
        p += 1;
        if data.get(p..p + 4) == Some(b".con") {
            return Some(p + 4);
        }
    }
    None
}

fn read_construction(data: &[u8], start: usize, end: usize) -> Option<Construction> {
    let u32_at = |p: usize| {
        data.get(p..p + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    if start < 4 || u32_at(start - 4)? as usize != end - start {
        return None;
    }
    let mut r = Reader::at(data, end);
    r.skip_table().ok()?;
    let q = r.pos();
    let f = |i: usize| {
        data.get(q + 4 * i..q + 4 * i + 4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    };
    if f(3)? != 0.0 || f(7)? != 0.0 || f(11)? != 0.0 || f(15)? != 1.0 {
        return None;
    }
    let (x, y) = (f64::from(f(12)?), f64::from(f(13)?));
    let mut p = q + 64 + 8; // the transform, then an i64
    let mut ids = Vec::new();
    for _ in 0..7 {
        let n = u32_at(p)? as usize;
        if n > IDS_MAX {
            return None;
        }
        let list = data.get(p + 4..p + 4 + 4 * n)?;
        ids.extend(
            list.as_chunks::<4>()
                .0
                .iter()
                .map(|b| u32::from_le_bytes(*b)),
        );
        p += 4 + 4 * n;
    }
    let path = std::str::from_utf8(&data[start..end]).ok()?;
    Some(Construction {
        key: path.split('/').nth(1)?.to_owned(),
        x,
        y,
        ids,
    })
}

fn read_constructions(data: &[u8]) -> Vec<Construction> {
    let mut out = Vec::new();
    let mut next_free = 0;
    for at in memchr::memmem::find_iter(data, CON_START) {
        if at < next_free {
            continue;
        }
        let Some(end) = con_path_end(data, at) else {
            continue;
        };
        next_free = end;
        if let Some(c) = read_construction(data, at, end) {
            out.push(c);
        }
    }
    out
}

fn near(ax: f64, ay: f64, bx: f64, by: f64) -> bool {
    (ax - bx).hypot(ay - by) <= SAME_SPOT
}

/// Remove from `instances` the main instances of deleted industries and of
/// older record copies; `mains` maps a main model's path to its industry key
/// ([`crate::industry_models`]). `free` is the free-id list. Returns how many
/// were removed. The industry's other pieces (fences, sheds, fields) stay: no
/// link from a piece to its record is known.
pub fn retain_standing_industries(
    data: &[u8],
    free: &HashSet<u32>,
    mains: &HashMap<String, String>,
    instances: &mut Vec<Instance>,
) -> usize {
    if free.is_empty() {
        return 0;
    }
    let recs = read_constructions(data);
    let nfree = |c: &Construction| c.ids.iter().filter(|i| free.contains(i)).count();
    let live: Vec<(&Construction, HashSet<u32>)> = recs
        .iter()
        .filter(|c| !c.ids.is_empty() && nfree(c) == 0)
        .map(|c| (c, c.ids.iter().copied().collect()))
        .collect();
    let (mut dead, mut copies) = (Vec::new(), Vec::new());
    for c in &recs {
        let k = nfree(c);
        if c.ids.is_empty() || k == 0 {
            continue;
        }
        if k == c.ids.len() {
            dead.push(c);
        } else if live.iter().any(|(l, ids)| {
            l.key == c.key
                && near(l.x, l.y, c.x, c.y)
                && c.ids.iter().all(|i| free.contains(i) || ids.contains(i))
        }) {
            copies.push(c);
        }
    }
    let mut removed = 0;
    let mut remove_one = |r: &Construction, min_left: usize| {
        let mut hits = 0;
        let mut nearest: Option<(usize, f64)> = None;
        for (i, inst) in instances.iter().enumerate() {
            if mains.get(&inst.model).is_some_and(|k| *k == r.key) && near(inst.x, inst.y, r.x, r.y)
            {
                hits += 1;
                let d = (inst.x - r.x).hypot(inst.y - r.y);
                if nearest.is_none_or(|(_, b)| d < b) {
                    nearest = Some((i, d));
                }
            }
        }
        if hits > min_left
            && let Some((i, _)) = nearest
        {
            instances.remove(i);
            removed += 1;
        }
    };
    for r in dead {
        remove_one(r, 0);
    }
    for r in copies {
        remove_one(r, 1);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(out: &mut Vec<u8>, key: &str, x: f32, y: f32, ids: &[u32]) {
        let path = format!("industries/{key}.con");
        out.extend((path.len() as u32).to_le_bytes());
        out.extend(path.as_bytes());
        out.extend(0u32.to_le_bytes()); // an empty table body
        for i in 0..16 {
            let v = match i {
                0 | 5 | 10 | 15 => 1.0,
                12 => x,
                13 => y,
                _ => 0.0,
            };
            out.extend(f32::to_le_bytes(v));
        }
        out.extend(0i64.to_le_bytes());
        for list in 0..7 {
            let own: &[u32] = if list == 0 { ids } else { &[] };
            out.extend((own.len() as u32).to_le_bytes());
            out.extend(own.iter().flat_map(|i| i.to_le_bytes()));
        }
    }

    fn main_at(x: f64, y: f64) -> Instance {
        Instance {
            slot: 0,
            model: "industries/farm/farm_sm.mdl".into(),
            x,
            y,
            z: 0.0,
            fx: 1.0,
            fy: 0.0,
        }
    }

    #[test]
    fn deleted_records_and_older_copies_drop_main_instances() {
        let mut data = vec![0u8; 8];
        record(&mut data, "farm/farm", 10.0, 10.0, &[1, 2]); // deleted
        record(&mut data, "farm/farm", 500.0, 0.0, &[3, 4]); // live
        record(&mut data, "farm/farm", 500.0, 0.0, &[3, 9]); // older copy of it
        record(&mut data, "farm/farm", 900.0, 0.0, &[5, 6]); // live, alone
        record(&mut data, "farm/farm", 900.0, 0.0, &[7, 8]); // free ids only: deleted
        let free: HashSet<u32> = [1, 2, 9, 7, 8].into();
        let mains = HashMap::from([("industries/farm/farm_sm.mdl".to_owned(), "farm".to_owned())]);
        let mut instances = vec![
            main_at(10.0, 10.0),
            main_at(500.0, 0.0),
            main_at(500.0, 0.0),
            main_at(900.0, 0.0),
            main_at(900.0, 0.0),
        ];
        // Two deleted, one copy over two instances; the lone live record
        // keeps the instance the deleted record at its spot did not take.
        let removed = retain_standing_industries(&data, &free, &mains, &mut instances);
        assert_eq!(removed, 3);
        let spots: Vec<_> = instances.iter().map(|i| (i.x, i.y)).collect();
        assert_eq!(spots, [(500.0, 0.0), (900.0, 0.0)]);
        // Without a free-id list nothing is dropped.
        let mut all = vec![main_at(10.0, 10.0)];
        assert_eq!(
            retain_standing_industries(&data, &HashSet::new(), &mains, &mut all),
            0
        );
    }
}
