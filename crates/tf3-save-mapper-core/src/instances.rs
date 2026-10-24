//! Model instances: where each placed model stands (`docs/models.md` in
//! tf3-save-format).
//!
//! The model table comes right after the header: a u32 count, then per model
//! a `str` source mod, a `str` path and a u32 id that starts at 0 and rises
//! along the table (it skips ahead where a mod's models are). An instance is
//! a model id followed by a 4 x 4 f32 transform whose floats 3, 7 and 11 are
//! 0 and float 15 is 1; it is found by that pattern after the table.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use memchr::memmem;

use crate::error::{Error, Result};

/// One instance record: the model id and the 16 floats.
const RECORD_BYTES: usize = 4 + 64;
/// 1.0 as f32: the transform's last float.
const ONE: [u8; 4] = [0x00, 0x00, 0x80, 0x3f];
/// The table is searched for this far before its first `animal/` path.
const TABLE_REACH: usize = 2000;
/// Fewest records in a run that counts as a page of the editor's preview block.
const BLOCK_RUN: usize = 6;

/// The model table: each model's path by its id.
#[derive(Debug, Clone)]
pub struct ModelTable {
    pub paths: BTreeMap<u32, String>,
    /// Offset just after the table.
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// The instance's place among all instance records after the model
    /// table, in stream order, whatever prefixes were asked for; only good
    /// within one save.
    pub slot: usize,
    pub model: String,
    /// The translation, metres.
    pub x: f64,
    pub y: f64,
    /// The first row of the transform: the direction the model's x axis points.
    pub fx: f64,
    pub fy: f64,
}

impl Instance {
    /// A point given in the model's own axes (a along its x axis, b along
    /// its y axis), on the map.
    pub fn place(&self, a: f64, b: f64) -> (f64, f64) {
        let n = self.fx.hypot(self.fy);
        let (fx, fy) = if n > 0.0 {
            (self.fx / n, self.fy / n)
        } else {
            (1.0, 0.0)
        };
        (self.x + a * fx - b * fy, self.y + a * fy + b * fx)
    }
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(at..at + 4)?.try_into().unwrap(),
    ))
}

/// Read the model table. It is found from the first `animal/` path, which the
/// base game's table sorts first.
pub fn read_model_table(data: &[u8]) -> Result<ModelTable> {
    let first = memmem::find(data, b"animal/").ok_or(Error::NoModelTable)?;
    (first.saturating_sub(TABLE_REACH)..first)
        .find_map(|start| table_at(data, start))
        .ok_or(Error::NoModelTable)
}

fn table_at(data: &[u8], start: usize) -> Option<ModelTable> {
    let n = u32_at(data, start)?;
    if !(101..100_000).contains(&n) {
        return None;
    }
    let mut p = start + 4;
    let mut paths = BTreeMap::new();
    let mut last: Option<u32> = None;
    for k in 0..n {
        let mod_len = u32_at(data, p)? as usize;
        p = p.checked_add(4 + mod_len)?;
        let len = u32_at(data, p)? as usize;
        let path = std::str::from_utf8(data.get(p + 4..p.checked_add(4 + len)?)?).ok()?;
        p += 4 + len;
        let id = u32_at(data, p)?;
        if id < k || last.is_some_and(|l| id <= l) || (k == 0 && id != 0) {
            return None;
        }
        p += 4;
        paths.insert(id, path.to_owned());
        last = Some(id);
    }
    Some(ModelTable { paths, end: p })
}

/// A candidate record at `at`: the model id, and the 16 floats.
fn record(data: &[u8], at: usize) -> (u32, [f32; 16]) {
    let id = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    let mut m = [0f32; 16];
    for (i, f) in m.iter_mut().enumerate() {
        let o = at + 4 + 4 * i;
        *f = f32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    }
    (id, m)
}

/// Read the instances whose model path starts with one of `prefixes`. The
/// model preview block of map editor saves (a copy of many models laid out
/// near 0,0, `docs/editor-saves.md`) is left out.
pub fn read_instances(data: &[u8], table: &ModelTable, prefixes: &[&str]) -> Vec<Instance> {
    let wanted: HashMap<u32, &str> = table
        .paths
        .iter()
        .filter(|(_, p)| prefixes.iter().any(|x| p.starts_with(x)))
        .map(|(&id, p)| (id, p.as_str()))
        .collect();
    let matches = find_records(data, table.end);
    let skip = preview_block(data, &matches);
    let mut out = Vec::new();
    for (k, &at) in matches.iter().enumerate() {
        let (id, m) = record(data, at);
        let Some(model) = wanted.get(&id) else {
            continue;
        };
        if skip.contains(&k) {
            continue;
        }
        let (x, y, z) = (m[12], m[13], m[14]);
        let (fx, fy) = (m[0], m[1]);
        if ![x, y, z, fx, fy].iter().all(|v| v.is_finite()) {
            continue;
        }
        out.push(Instance {
            slot: k,
            model: (*model).to_owned(),
            x: f64::from(x),
            y: f64::from(y),
            fx: f64::from(fx),
            fy: f64::from(fy),
        });
    }
    out
}

/// Offsets of every record after `from`, in stream order and not
/// overlapping: an id, then a transform whose floats 3, 7 and 11 are zero
/// and whose float 15 is 1.0.
fn find_records(data: &[u8], from: usize) -> Vec<usize> {
    let Some(tail) = data.get(from..) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut next_free = 0; // records do not overlap
    for one in memmem::find_iter(tail, &ONE) {
        let Some(start) = one.checked_sub(RECORD_BYTES - 4) else {
            continue;
        };
        if start < next_free {
            continue;
        }
        let zero = |o: usize| tail[start + o..start + o + 4] == [0; 4];
        if zero(16) && zero(32) && zero(48) {
            out.push(from + start);
            next_free = start + RECORD_BYTES;
        }
    }
    out
}

/// The indexes (into `matches`) of the records of the editor's model preview
/// block. A page is a run of records exactly 68 bytes apart whose transform
/// has no rotation (only the diagonal and the translation are non-zero) and
/// whose model ids rise by one. The block is pages of at least `BLOCK_RUN`
/// records, and a shorter page joins it when it sits next to a block page
/// with at most 200 bytes between them.
fn preview_block(data: &[u8], matches: &[usize]) -> BTreeSet<usize> {
    let mut pages: Vec<Vec<usize>> = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let mut prev: Option<(usize, u32)> = None;
    for (k, &at) in matches.iter().enumerate() {
        let (id, m) = record(data, at);
        let plain = [1, 2, 4, 6, 8, 9].iter().all(|&i| m[i] == 0.0);
        if plain && prev.is_some_and(|(p, pid)| at - p == RECORD_BYTES && id == pid.wrapping_add(1))
        {
            run.push(k);
        } else {
            if !run.is_empty() {
                pages.push(std::mem::take(&mut run));
            }
            if plain {
                run.push(k);
            }
        }
        prev = Some((at, id));
    }
    if !run.is_empty() {
        pages.push(run);
    }
    let mut keep: Vec<bool> = pages.iter().map(|p| p.len() >= BLOCK_RUN).collect();
    let near = |a: usize, b: usize| {
        let gap = matches[pages[b][0]] as i64
            - matches[*pages[a].last().unwrap()] as i64
            - RECORD_BYTES as i64;
        0 < gap && gap <= 200
    };
    for i in 1..pages.len() {
        keep[i] = keep[i] || (keep[i - 1] && near(i - 1, i));
    }
    for i in (0..pages.len().saturating_sub(1)).rev() {
        keep[i] = keep[i] || (keep[i + 1] && near(i, i + 1));
    }
    pages
        .into_iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .flat_map(|(p, _)| p)
        .collect()
}

/// {model path: industry key}: the main model of each industry folder, which
/// stands for one industry. That is `industries/<key>/<key>_sm.mdl`, else the
/// first `_particles.mdl` of the folder. When a main model has no instance
/// but the key's own `<key>.mdl` has, that one stands in.
pub fn industry_models(table: &ModelTable, instances: &[Instance]) -> HashMap<String, String> {
    let paths: BTreeSet<&str> = table.paths.values().map(String::as_str).collect();
    let keys: BTreeSet<&str> = paths
        .iter()
        .filter(|p| p.starts_with("industries/") && p.matches('/').count() >= 2)
        .map(|p| p.split('/').nth(1).unwrap())
        .collect();
    let placed: BTreeSet<&str> = instances.iter().map(|i| i.model.as_str()).collect();
    let mut out = HashMap::new();
    for key in keys {
        let sm = format!("industries/{key}/{key}_sm.mdl");
        let main = if paths.contains(sm.as_str()) {
            Some(sm)
        } else {
            let folder = format!("industries/{key}/");
            paths
                .iter()
                .find(|p| p.starts_with(&folder) && p.ends_with("_particles.mdl"))
                .map(|p| (*p).to_owned())
        };
        let Some(main) = main else {
            continue;
        };
        let own = format!("industries/{key}/{key}.mdl");
        if !placed.contains(main.as_str()) && placed.contains(own.as_str()) {
            out.insert(own, key.to_owned());
        }
        out.insert(main, key.to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
    }

    fn instance(out: &mut Vec<u8>, id: u32, x: f32, y: f32, facing: (f32, f32)) {
        out.extend_from_slice(&id.to_le_bytes());
        let m = [
            facing.0, facing.1, 0.0, 0.0, -facing.1, facing.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y,
            5.0, 1.0,
        ];
        for f in m {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }

    fn stream() -> Vec<u8> {
        let mut s = vec![7u8; 10];
        let mut paths: Vec<String> = (0..120).map(|i| format!("animal/a{i}.mdl")).collect();
        paths.push("industries/farm/farm_sm.mdl".into());
        paths.push("depots/road/road_depot/road_depot.mdl".into());
        s.extend_from_slice(&(paths.len() as u32).to_le_bytes());
        for (i, p) in paths.iter().enumerate() {
            string(&mut s, "");
            string(&mut s, p);
            // A mod's models skip ahead: the last id is not its index.
            let id = if i == paths.len() - 1 { 500 } else { i as u32 };
            s.extend_from_slice(&id.to_le_bytes());
        }
        s.extend_from_slice(&[9; 5]);
        instance(&mut s, 120, 100.0, -50.0, (0.0, 1.0));
        s.extend_from_slice(&[3; 7]);
        instance(&mut s, 500, -20.0, 30.0, (1.0, 0.0));
        s
    }

    #[test]
    fn instances_are_found_after_the_table() {
        let s = stream();
        let table = read_model_table(&s).unwrap();
        assert_eq!(table.paths.len(), 122);
        assert_eq!(table.paths[&500], "depots/road/road_depot/road_depot.mdl");
        let found = read_instances(&s, &table, &["industries/", "depots/"]);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].model, "industries/farm/farm_sm.mdl");
        assert_eq!((found[0].x, found[0].y), (100.0, -50.0));
        assert_eq!((found[0].slot, found[1].slot), (0, 1));
        // Facing north: the model's x axis points up the map.
        let (x, y) = found[0].place(10.0, 0.0);
        assert!((x - 100.0).abs() < 1e-9 && (y - -40.0).abs() < 1e-9);
        let mains = industry_models(&table, &found);
        assert_eq!(mains["industries/farm/farm_sm.mdl"], "farm");
    }

    #[test]
    fn the_editor_preview_block_is_left_out() {
        let mut s = stream();
        let table = read_model_table(&s).unwrap();
        // Six plain records 68 bytes apart with rising ids: a page of the block.
        for id in 115..121 {
            instance(&mut s, id, 0.0, 0.0, (1.0, 0.0));
        }
        let found = read_instances(&s, &table, &["industries/"]);
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
