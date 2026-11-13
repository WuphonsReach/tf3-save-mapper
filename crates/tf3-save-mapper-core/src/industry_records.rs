//! Industries deleted in the map editor, the older copies of an industry's
//! record that stack at one spot, and the slots of each industry's fields.
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
//! - Replaced: the editor's regenerate leaves the old set's records and
//!   instances, and frees their ids; the game hands those ids out again as it
//!   builds, so after a while few of them are still free. What stays true: a
//!   land industry's first list holds an end node of its own road, a few dozen
//!   metres off. A record is replaced when its first list holds ids and none
//!   ends an edge within 500 m, or when any of its ids ends an edge and none of
//!   that node's ends lies within 500 m (the id now names another road's
//!   node). Drop one main instance of the key within 1 m, while more are left
//!   at that spot than live records stand there.
//!
//! Fields: the body's `modules` table is keyed by slot number, one entry per
//! field (a farm's plots, an oil well's pumpjacks, a mine's pits); the slots'
//! places are in the industry's `.con.lua` in the install
//! ([`crate::install::industry_fields`]).

use std::collections::{HashMap, HashSet};

use crate::edges::Edge;
use crate::instances::Instance;
use crate::reader::{Reader, Scalar};

/// Spots closer than this, metres, are the same spot.
const SAME_SPOT: f64 = 1.0;
/// An industry's own road ends this close to its record, metres (59 to 107
/// seen); an id ending an edge only farther off is not the record's any more.
const OWN_ROAD: f64 = 500.0;
/// A longer id list is not a record's.
const IDS_MAX: usize = 100_000;
const CON_START: &[u8] = b"industries/";

struct Construction {
    key: String,
    x: f64,
    y: f64,
    /// The first list: a land industry's road node.
    first: Vec<u32>,
    /// All seven lists.
    ids: Vec<u32>,
    /// The keys of the body's `modules` table: the slots of its fields.
    slots: Vec<u32>,
}

/// A field slot's number is small; anything else in `modules` is not a slot.
const SLOT_MAX: f64 = 1000.0;

/// The keys of the `modules` table at the top of a record's body, with the
/// cursor left after the body.
fn body_slots(r: &mut Reader) -> crate::Result<Vec<u32>> {
    let mut slots = Vec::new();
    for _ in 0..r.u32()? {
        if r.scalar()? != Scalar::Text("modules".into()) {
            r.skip_value()?;
            continue;
        }
        let Some(n) = r.table_start()? else {
            continue;
        };
        for _ in 0..n {
            if let Scalar::Number(k) = r.scalar()?
                && k.fract() == 0.0
                && (1.0..=SLOT_MAX).contains(&k)
            {
                slots.push(k as u32);
            }
            r.skip_value()?;
        }
    }
    Ok(slots)
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
    let slots = body_slots(&mut r).ok()?;
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
    let mut first = Vec::new();
    for nth in 0..7 {
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
        if nth == 0 {
            first.clone_from(&ids);
        }
        p += 4 + 4 * n;
    }
    let path = std::str::from_utf8(&data[start..end]).ok()?;
    Some(Construction {
        key: path.split('/').nth(1)?.to_owned(),
        x,
        y,
        first,
        ids,
        slots,
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

/// Remove from `instances` the main instances of deleted industries, of older
/// record copies and of industries a regenerate replaced; `mains` maps a main
/// model's path to its industry key ([`crate::industry_models`]). `free` is
/// the free-id list, `edges` the network with the dead edges already left out.
/// Returns how many were removed. The industry's other pieces (fences, sheds,
/// a field's models) stay: no link from a piece to its record is known.
pub fn retain_standing_industries(
    data: &[u8],
    free: &HashSet<u32>,
    mains: &HashMap<String, String>,
    edges: &[Edge],
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
    for r in &copies {
        remove_one(r, 1);
    }
    // Replaced: only the records nothing above took. With no network to test
    // against every road node would look missing, so nothing is dropped.
    if edges.is_empty() {
        return removed;
    }
    let mut ends: HashMap<u32, Vec<(f64, f64)>> = HashMap::new();
    for e in edges {
        for (id, p) in [(e.a, e.p0), (e.b, e.p1)] {
            ends.entry(id)
                .or_default()
                .push((f64::from(p[0]), f64::from(p[1])));
        }
    }
    let home = |c: &Construction, id: &u32| {
        ends.get(id).map(|ps| {
            ps.iter()
                .any(|&(x, y)| (x - c.x).hypot(y - c.y) <= OWN_ROAD)
        })
    };
    let replaced = |c: &Construction| {
        (!c.first.is_empty() && !c.first.iter().any(|i| home(c, i) == Some(true)))
            || c.ids.iter().any(|i| home(c, i) == Some(false))
    };
    let standing: Vec<&Construction> = live
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| !replaced(c))
        .collect();
    for c in &recs {
        let k = nfree(c);
        let taken = k == c.ids.len() || copies.iter().any(|r| std::ptr::eq(*r, c));
        if !c.ids.is_empty() && !taken && replaced(c) {
            let left = standing
                .iter()
                .filter(|s| s.key == c.key && near(s.x, s.y, c.x, c.y))
                .count();
            remove_one(c, left);
        }
    }
    removed
}

/// The field slots of each industry in `instances`, by the main instance's
/// `data-slot`: the keys of the `modules` table of its construction record,
/// the record of the same key within 1 m. Where several records share the
/// spot (older copies, a regenerate's leftovers) the one with the fewest ids
/// in `free` wins, then one with fields, then the first in the stream.
/// Industries whose record has no fields are left out.
pub fn field_slots(
    data: &[u8],
    free: Option<&HashSet<u32>>,
    mains: &HashMap<String, String>,
    instances: &[Instance],
) -> HashMap<usize, Vec<u32>> {
    let recs = read_constructions(data);
    let nfree =
        |c: &Construction| free.map_or(0, |f| c.ids.iter().filter(|i| f.contains(i)).count());
    let mut out = HashMap::new();
    for inst in instances {
        let Some(key) = mains.get(&inst.model) else {
            continue;
        };
        let best = recs
            .iter()
            .filter(|c| c.key == *key && near(c.x, c.y, inst.x, inst.y))
            .min_by_key(|c| (nfree(c), c.slots.is_empty()));
        if let Some(c) = best
            && !c.slots.is_empty()
        {
            out.insert(inst.slot, c.slots.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(out: &mut Vec<u8>, key: &str, x: f32, y: f32, ids: &[u32]) {
        record_lists(out, key, x, y, ids, &[]);
    }

    /// A record with `first` in its first list and `rest` in its fourth.
    fn record_lists(out: &mut Vec<u8>, key: &str, x: f32, y: f32, first: &[u32], rest: &[u32]) {
        record_full(out, key, x, y, first, rest, None);
    }

    fn lua_text(out: &mut Vec<u8>, s: &str) {
        out.extend(3u32.to_le_bytes());
        out.extend((s.len() as u32).to_le_bytes());
        out.extend(s.as_bytes());
    }

    fn lua_number(out: &mut Vec<u8>, v: f64) {
        out.extend(2u32.to_le_bytes());
        out.extend(v.to_le_bytes());
    }

    /// A body as the game writes one, keys in sorted order: `modules` keyed
    /// by slot (each a table naming its module file), then `seed` and `year`.
    fn body_with_modules(out: &mut Vec<u8>, slots: &[u32]) {
        out.extend(3u32.to_le_bytes()); // three pairs
        lua_text(out, "modules");
        out.extend(4u32.to_le_bytes());
        out.push(1);
        out.extend((slots.len() as u32).to_le_bytes());
        for &n in slots {
            lua_number(out, f64::from(n));
            out.extend(4u32.to_le_bytes());
            out.push(1);
            out.extend(2u32.to_le_bytes());
            lua_text(out, "name");
            lua_text(out, "::/industries/farm/field.module");
            lua_text(out, "variant");
            lua_number(out, 0.0);
        }
        lua_text(out, "seed");
        lua_number(out, 1234.0);
        lua_text(out, "year");
        lua_number(out, 1900.0);
    }

    fn record_full(
        out: &mut Vec<u8>,
        key: &str,
        x: f32,
        y: f32,
        first: &[u32],
        rest: &[u32],
        slots: Option<&[u32]>,
    ) {
        let path = format!("industries/{key}.con");
        out.extend((path.len() as u32).to_le_bytes());
        out.extend(path.as_bytes());
        match slots {
            Some(slots) => body_with_modules(out, slots),
            None => out.extend(0u32.to_le_bytes()), // an empty table body
        }
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
        for nth in 0..7 {
            let own: &[u32] = match nth {
                0 => first,
                3 => rest,
                _ => &[],
            };
            out.extend((own.len() as u32).to_le_bytes());
            out.extend(own.iter().flat_map(|i| i.to_le_bytes()));
        }
    }

    /// A road from node `a` at `from` to node `b` at `to`.
    fn road(a: u32, b: u32, from: (f32, f32), to: (f32, f32)) -> Edge {
        Edge {
            a,
            b,
            p0: [from.0, from.1, 0.0],
            p1: [to.0, to.1, 0.0],
            t0: [0.0; 3],
            t1: [0.0; 3],
            template: Some("street/constructions/ind_entrance_old_country".into()),
            family: crate::edges::Family::Street,
            width: Some(8.0),
            modes: None,
            offset: 0,
        }
    }

    fn main_at(x: f64, y: f64) -> Instance {
        main_of("farm", x, y)
    }

    fn main_of(key: &str, x: f64, y: f64) -> Instance {
        Instance {
            slot: 0,
            model: format!("industries/{key}/{key}_sm.mdl"),
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
        let removed = retain_standing_industries(&data, &free, &mains, &[], &mut instances);
        assert_eq!(removed, 3);
        let spots: Vec<_> = instances.iter().map(|i| (i.x, i.y)).collect();
        assert_eq!(spots, [(500.0, 0.0), (900.0, 0.0)]);
        // Without a free-id list nothing is dropped.
        let mut all = vec![main_at(10.0, 10.0)];
        assert_eq!(
            retain_standing_industries(&data, &HashSet::new(), &mains, &[], &mut all),
            0
        );
    }

    #[test]
    fn regenerated_records_drop_main_instances() {
        let mut data = vec![0u8; 8];
        // Live: its road node 1 ends its entrance road 90 m off.
        record_lists(&mut data, "farm/farm", 0.0, 0.0, &[1], &[2]);
        // Live, no road (an oil platform): nothing to test it by.
        record_lists(
            &mut data,
            "oil_platform/oil_platform",
            3000.0,
            0.0,
            &[],
            &[3, 4],
        );
        // Old set: road node 20 ends no edge any more.
        record_lists(&mut data, "farm/farm", 1000.0, 0.0, &[20], &[21]);
        // Old set: id 30 is now a node of a country road 5 km away.
        record_lists(
            &mut data,
            "oil_platform/oil_platform",
            -2000.0,
            0.0,
            &[],
            &[30, 31],
        );
        // Old set at a live farm's spot: the farm's one instance stays.
        record_lists(&mut data, "farm/farm", 0.0, 0.0, &[40], &[41]);
        let free: HashSet<u32> = [99].into();
        let edges = [
            road(1, 5, (90.0, 0.0), (300.0, 0.0)),
            road(30, 6, (3000.0, 4000.0), (3100.0, 4000.0)),
        ];
        let mains = HashMap::from([
            ("industries/farm/farm_sm.mdl".to_owned(), "farm".to_owned()),
            (
                "industries/oil_platform/oil_platform_sm.mdl".to_owned(),
                "oil_platform".to_owned(),
            ),
        ]);
        let mut instances = vec![
            main_at(0.0, 0.0),
            main_of("oil_platform", 3000.0, 0.0),
            main_at(1000.0, 0.0),
            main_of("oil_platform", -2000.0, 0.0),
        ];
        let removed = retain_standing_industries(&data, &free, &mains, &edges, &mut instances);
        assert_eq!(removed, 2);
        let spots: Vec<_> = instances.iter().map(|i| (i.x, i.y)).collect();
        assert_eq!(spots, [(0.0, 0.0), (3000.0, 0.0)]);
        // With no network to test against, nothing is taken as replaced.
        let mut all = vec![main_at(1000.0, 0.0)];
        assert_eq!(
            retain_standing_industries(&data, &free, &mains, &[], &mut all),
            0
        );
    }

    #[test]
    fn field_slots_come_from_the_record_at_the_industry() {
        let mut data = vec![0u8; 8];
        // An older copy (id 9 free) with one field more, then the live record.
        record_full(
            &mut data,
            "farm/farm",
            0.0,
            0.0,
            &[1],
            &[9],
            Some(&[2, 5, 7]),
        );
        record_full(&mut data, "farm/farm", 0.0, 0.0, &[1], &[3], Some(&[2, 5]));
        // A record with no fields beside one with fields, at another farm.
        record_full(&mut data, "farm/farm", 500.0, 0.0, &[4], &[], None);
        record_full(&mut data, "farm/farm", 500.0, 0.0, &[5], &[], Some(&[20]));
        // An oil well's record 2 m from the farm: another spot.
        record_full(
            &mut data,
            "oil_well/oil_well",
            902.0,
            0.0,
            &[6],
            &[],
            Some(&[1]),
        );
        let mains = HashMap::from([
            ("industries/farm/farm_sm.mdl".to_owned(), "farm".to_owned()),
            (
                "industries/oil_well/oil_well_sm.mdl".to_owned(),
                "oil_well".to_owned(),
            ),
        ]);
        let mut instances = vec![
            main_at(0.0, 0.0),
            main_at(500.0, 0.0),
            main_of("oil_well", 900.0, 0.0),
        ];
        for (n, i) in instances.iter_mut().enumerate() {
            i.slot = 10 + n;
        }
        let free: HashSet<u32> = [9].into();
        let slots = field_slots(&data, Some(&free), &mains, &instances);
        assert_eq!(slots.get(&10), Some(&vec![2, 5]));
        assert_eq!(slots.get(&11), Some(&vec![20]));
        assert_eq!(slots.get(&12), None);
        // With no free-id list the first record at the spot wins.
        let slots = field_slots(&data, None, &mains, &instances);
        assert_eq!(slots.get(&10), Some(&vec![2, 5, 7]));
        // The body is read through: the record's lists still come out.
        let recs = read_constructions(&data);
        assert_eq!(recs.len(), 5);
        assert_eq!(recs[1].ids, [1, 3]);
    }
}
