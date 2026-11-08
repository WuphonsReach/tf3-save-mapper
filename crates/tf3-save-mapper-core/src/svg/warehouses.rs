//! The `warehouses` layer, under the roads: each warehouse's ground in
//! orange with the black outline, and inside it, in thin dark lines, what
//! each module holds, so the cargo class of every place in the complex shows.
//!
//! A warehouse is one construction (`warehouses/warehouse.con`) of an
//! entrance and 1 to 15 modules (`docs/warehouses.md` in tf3-save-format).
//! The save keeps the construction's own models as instances, each module's
//! at the module's centre, so the pieces give every module's place, turn and
//! class. The ground is the union of the faces the warehouse script lays:
//! a 40 m square round each module's centre (modules sit 32 m apart, so the
//! squares of neighbours overlap) and a 40 by 24 m face round the
//! construction's origin, which lies 3 m beyond the entrance model. Each
//! module centre lies on the construction's 8 m grid; a warehouse is grown
//! from its entrance over centres whose faces meet and that share its grid,
//! and a module standing apart joins the entrance on whose grid it lies.
//! Some entrances have no turn of their own; they take their modules'.
//! Module pieces whose facing is not a unit vector are left out (the
//! reader already drops the preview block's tilted pages, which hold every
//! warehouse piece; this keeps any such record that gets through off the map).
//!
//! The marks inside are the pieces' boxes and cargo bays from the model
//! files in the install: a goods module's hall (its four corner pieces as one
//! shape, so connected halls run on), a bulk module's two bunkers, a liquid
//! module's round tank and the pipes to its neighbours, a flatbed module's
//! crane rails and crane, a universal module's open yard (dashed) and shack.
//! Without the install the ground is drawn with no marks; each module's
//! class is still in its `data-type`.

use std::collections::HashMap;

use super::{Cut, OUTLINE, Picture, escape, fmt_g, fmt1, fmt2, layer, loops_d, runs_d};
use crate::geometry::{closed_outline, outer_runs};
use crate::install::{Rect, WarehouseModel};
use crate::instances::Instance;

/// The entrance model, one per warehouse.
pub const ENTRANCE: &str = "warehouses/entrance/entrance_sm.mdl";
const GROUND_FILL: &str = "#f4b26a";
const INK: &str = "#2b1a08";
/// The marks are drawn this opaque, as one layer over the ground.
const INK_OPACITY: f64 = 0.65;
/// Pixels: the width of the marks inside, at most half the outline's.
const DETAIL_PX: f64 = 0.5;
/// Metres each way from a module's centre to the edge of its ground face
/// (the script's half size of 16 m and half its 8 m cell).
const MODULE_HALF: f64 = 20.0;
/// The entrance's ground face in the entrance model's axes: 20 m each way
/// along x, and 12 m each way along y round the construction's origin, 3 m
/// beyond the model.
const ENTRANCE_FACE: Rect = (-20.0, -9.0, 20.0, 15.0);
/// Metres: the warehouse script's grid. Every module's centre lies on it,
/// counted from the construction's origin in the construction's axes, at
/// most `GRID_CELLS` cells out along each axis.
const GRID: f64 = 8.0;
const GRID_CELLS: f64 = 20.0;
/// Metres a centre may lie off the grid and still count as on it.
const GRID_SLACK: f64 = 0.5;
/// Metres: faces this close count as meeting.
const MEET: f64 = 0.5;
/// Metres: gaps up to twice this inside one warehouse's ground are closed.
const GROUND_CLOSE: f64 = 0.5;
/// Metres along a bunker's side between its hatch lines.
const HATCH_STEP: f64 = 4.0;
/// Two pieces are turned the same way (or a half turn aside, as alternate
/// bulk and liquid modules are) when the sine between them is below this.
const SAME_TURN: f64 = 0.02;

/// A module's cargo class, from its pieces' folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Bulk,
    Flatbed,
    Goods,
    Liquid,
    Universal,
}

impl Class {
    fn name(self) -> &'static str {
        match self {
            Class::Bulk => "bulk",
            Class::Flatbed => "flatbed",
            Class::Goods => "goods",
            Class::Liquid => "liquid",
            Class::Universal => "universal",
        }
    }
}

fn class(model: &str) -> Option<Class> {
    let rest = model.strip_prefix("warehouses/")?;
    let (folder, _) = rest.split_once('/')?;
    match folder {
        "wh_bulk" => Some(Class::Bulk),
        "wh_flatbed" => Some(Class::Flatbed),
        "wh_goods" => Some(Class::Goods),
        "wh_liquid" => Some(Class::Liquid),
        "wh_universal" => Some(Class::Universal),
        _ => None,
    }
}

/// The file name of a piece without `.mdl`.
fn stem(model: &str) -> &str {
    let name = model.rsplit('/').next().unwrap_or(model);
    name.strip_suffix(".mdl").unwrap_or(name)
}

/// Whether a piece is the one every module of its class has once, at its
/// centre: a goods module's top right corner (in whichever of its joined
/// forms), and the other classes' main piece.
fn is_centre_piece(model: &str) -> bool {
    let s = stem(model);
    match class(model) {
        Some(Class::Goods) => s.starts_with("top") && s.contains("_right"),
        Some(Class::Bulk) => s == "wh_bulk_center",
        Some(Class::Flatbed) => s == "wh_flatbed_rails",
        Some(Class::Liquid) => s == "wh_liquid",
        Some(Class::Universal) => s == "wh_universal_sm",
        None => false,
    }
}

/// Whether an instance is drawn on the warehouses layer.
pub fn is_warehouse_piece(model: &str) -> bool {
    model == ENTRANCE || class(model).is_some()
}

/// How many warehouses and modules `rows` hold, grouped as they are drawn.
pub fn count(rows: &[Instance]) -> (usize, usize) {
    let found = warehouses(rows);
    let modules = found.iter().map(|w| w.modules.len()).sum();
    (found.len(), modules)
}

/// How a piece is marked inside its module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Its box, joined with the module's other boxes into one outline.
    Box,
    /// Each cargo bay's box, hatched.
    Bays,
    /// A circle filling its first cargo bay.
    Tank,
    /// A line along the middle of its box, the long way.
    Pipe,
    /// A line along each of its box's long sides.
    Rails,
    /// Its box, dashed.
    Yard,
}

fn mark(model: &str) -> Option<Mark> {
    let s = stem(model);
    match class(model)? {
        Class::Goods => Some(Mark::Box),
        Class::Bulk => (s == "wh_bulk_center").then_some(Mark::Bays),
        Class::Liquid if s == "wh_liquid" => Some(Mark::Tank),
        Class::Liquid => s.starts_with("wh_liquid_con_").then_some(Mark::Pipe),
        Class::Flatbed => match s {
            "wh_flatbed_rails" => Some(Mark::Rails),
            "wh_flatbed" => Some(Mark::Box),
            _ => None,
        },
        Class::Universal => match s {
            "wh_universal_sm" => Some(Mark::Yard),
            "wh_universal_shack" => Some(Mark::Box),
            _ => None,
        },
    }
}

/// A box's corners in picture metres.
fn corners(picture: &Picture, i: &Instance, (a0, b0, a1, b1): Rect) -> Vec<(f64, f64)> {
    [(a0, b0), (a1, b0), (a1, b1), (a0, b1)]
        .iter()
        .map(|&(a, b)| picture.pic(i.place(a, b)))
        .collect()
}

/// A ground face in the instance's own axes: its entrance face or its module square.
fn face(i: &Instance) -> Rect {
    if i.model == ENTRANCE {
        ENTRANCE_FACE
    } else {
        (-MODULE_HALF, -MODULE_HALF, MODULE_HALF, MODULE_HALF)
    }
}

/// The unit facing of an instance.
fn unit(i: &Instance) -> (f64, f64) {
    let n = i.fx.hypot(i.fy);
    if n > 0.0 {
        (i.fx / n, i.fy / n)
    } else {
        (1.0, 0.0)
    }
}

/// Whether the faces of two entrances or module centres meet: turned the
/// same way, and overlapping or within `MEET` in the first one's axes.
fn faces_meet(p: &Instance, q: &Instance) -> bool {
    let ((px, py), (qx, qy)) = (unit(p), unit(q));
    if (px * qy - py * qx).abs() > SAME_TURN {
        return false;
    }
    let centre = |i: &Instance| {
        let (a0, b0, a1, b1) = face(i);
        (
            i.place((a0 + a1) / 2.0, (b0 + b1) / 2.0),
            (a1 - a0) / 2.0,
            (b1 - b0) / 2.0,
        )
    };
    let ((pc, pa, pb), (qc, qa, qb)) = (centre(p), centre(q));
    let (dx, dy) = (qc.0 - pc.0, qc.1 - pc.1);
    let (da, db) = (dx * px + dy * py, -dx * py + dy * px);
    da.abs() <= pa + qa + MEET && db.abs() <= pb + qb + MEET
}

/// One module: its centre piece and the pieces that belong to it.
struct Module<'a> {
    centre: &'a Instance,
    class: Class,
    pieces: Vec<&'a Instance>,
}

/// One warehouse: its entrance, if found (with the turn it is drawn at),
/// and its modules, at least one.
struct Warehouse<'a> {
    entrance: Option<Instance>,
    modules: Vec<Module<'a>>,
}

/// Whether an instance's facing is a unit vector. Every module piece seen
/// on the map has one; the preview block's tilted pages at one save's map
/// centre (`instances::preview_block`) mostly do not.
fn turned(i: &Instance) -> bool {
    (i.fx.hypot(i.fy) - 1.0).abs() < 0.02
}

/// Whether `c` lies on the grid whose origin is `origin` in `p`'s axes,
/// turned as `p` is or a half turn aside, at most `cells` cells out along
/// each axis.
fn grid_fits(p: &Instance, origin: (f64, f64), c: &Instance, cells: f64) -> bool {
    let ((px, py), (cx, cy)) = (unit(p), unit(c));
    if (px * cy - py * cx).abs() > SAME_TURN {
        return false;
    }
    let (dx, dy) = (c.x - origin.0, c.y - origin.1);
    [dx * px + dy * py, -dx * py + dy * px].iter().all(|&d| {
        let k = (d / GRID).round();
        k.abs() <= cells && (d - k * GRID).abs() <= GRID_SLACK
    })
}

/// Whether module centre `c` lies on the grid of entrance `e` (turned).
fn on_grid(e: &Instance, c: &Instance) -> bool {
    grid_fits(e, e.place(0.0, 3.0), c, GRID_CELLS)
}

/// Whether two entrances or module centres could be of one warehouse: never
/// two entrances; an entrance and a centre on its grid; two centres on one
/// grid (as far apart as two centres on opposite edges of it).
fn same_grid(p: &Instance, q: &Instance) -> bool {
    match (p.model == ENTRANCE, q.model == ENTRANCE) {
        (true, true) => false,
        (true, false) => on_grid(p, q),
        (false, true) => on_grid(q, p),
        (false, false) => grid_fits(p, (p.x, p.y), q, 2.0 * GRID_CELLS),
    }
}

/// An entrance turned as its warehouse is. Some entrances keep their
/// position and an all-zero turn; such a one takes the turn of the nearest
/// module centre on whose grid it then stands, as that centre is turned or
/// a half turn aside (only one of the two puts the centre on the grid, the
/// origins lying 6 m apart). `None` when it has no turn and no centre fits.
fn turned_entrance(e: &Instance, centres: &[&Instance]) -> Option<Instance> {
    if turned(e) {
        return Some(e.clone());
    }
    let mut near: Vec<(&Instance, f64)> = centres
        .iter()
        .map(|&c| (c, (c.x - e.x).hypot(c.y - e.y)))
        .collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1));
    near.iter().find_map(|(c, _)| {
        let (fx, fy) = unit(c);
        [1.0, -1.0].iter().find_map(|sign| {
            let t = Instance {
                fx: sign * fx,
                fy: sign * fy,
                ..e.clone()
            };
            on_grid(&t, c).then_some(t)
        })
    })
}

/// The warehouses among `rows`. Every piece goes with the nearest module
/// centre of its class within a module's half size. Entrances and centres
/// are linked where their faces meet and they share a grid (never two
/// entrances), and each centre goes with the entrance it is fewest metres
/// from along the links: a warehouse built against another, side by side or
/// back to back, stays apart from it. A group of centres no entrance reaches
/// (modules can stand apart, their faces not meeting) goes with the nearest
/// entrance on whose grid it lies. Module pieces whose facing is not a unit
/// vector, and groups with no module, are left out. In the order of their
/// first instance in the stream.
fn warehouses(rows: &[Instance]) -> Vec<Warehouse<'_>> {
    let mut centres: Vec<&Instance> = rows
        .iter()
        .filter(|i| is_centre_piece(&i.model) && turned(i))
        .collect();
    centres.sort_by_key(|i| i.slot);
    let mut pieces: Vec<Vec<&Instance>> = vec![Vec::new(); centres.len()];
    for i in rows
        .iter()
        .filter(|i| mark(&i.model).is_some() && turned(i))
    {
        let c = class(&i.model);
        let nearest = centres
            .iter()
            .enumerate()
            .filter(|(_, m)| class(&m.model) == c)
            .map(|(k, m)| (k, (m.x - i.x).hypot(m.y - i.y)))
            .filter(|&(_, d)| d <= MODULE_HALF)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((k, _)) = nearest {
            pieces[k].push(i);
        }
    }
    let mut entrances: Vec<Instance> = rows
        .iter()
        .filter(|i| i.model == ENTRANCE)
        .filter_map(|e| turned_entrance(e, &centres))
        .collect();
    entrances.sort_by_key(|i| i.slot);
    // Entrances first, then the module centres.
    let nodes: Vec<&Instance> = entrances.iter().chain(centres.iter().copied()).collect();
    let (n, n_entrances) = (nodes.len(), entrances.len());
    let distance = |a: usize, b: usize| (nodes[a].x - nodes[b].x).hypot(nodes[a].y - nodes[b].y);
    let mut links: Vec<Vec<usize>> = vec![Vec::new(); n];
    for a in 0..n {
        for b in a + 1..n {
            if faces_meet(nodes[a], nodes[b]) && same_grid(nodes[a], nodes[b]) {
                links[a].push(b);
                links[b].push(a);
            }
        }
    }
    // Grow every warehouse from its entrance at once; an entrance has no
    // link to another, so each stays its own.
    let mut owner: Vec<Option<usize>> = (0..n).map(|k| (k < n_entrances).then_some(k)).collect();
    let mut reach: Vec<f64> = (0..n)
        .map(|k| if k < n_entrances { 0.0 } else { f64::INFINITY })
        .collect();
    let mut done = vec![false; n];
    while let Some(k) = (0..n)
        .filter(|&k| !done[k] && reach[k].is_finite())
        .min_by(|&a, &b| reach[a].total_cmp(&reach[b]).then(a.cmp(&b)))
    {
        done[k] = true;
        for &m in &links[k] {
            let r = reach[k] + distance(k, m);
            if !done[m] && r < reach[m] {
                reach[m] = r;
                owner[m] = owner[k];
            }
        }
    }
    // The centres no entrance reached, in linked groups (each keyed by its
    // first centre), each with the nearest entrance whose grid fits one of
    // them.
    for k in n_entrances..n {
        if owner[k].is_some() {
            continue;
        }
        let mut group = vec![k];
        let mut at = 0;
        while at < group.len() {
            for &m in &links[group[at]] {
                if owner[m].is_none() && !group.contains(&m) {
                    group.push(m);
                }
            }
            at += 1;
        }
        let entrance = group
            .iter()
            .flat_map(|&c| (0..n_entrances).map(move |e| (e, c)))
            .filter(|&(e, c)| on_grid(nodes[e], nodes[c]))
            .min_by(|&(a, c), &(b, d)| distance(a, c).total_cmp(&distance(b, d)))
            .map_or(k, |(e, _)| e);
        for c in group {
            owner[c] = Some(entrance);
        }
    }
    let mut groups: Vec<(usize, Warehouse)> = Vec::new();
    let mut at_owner: HashMap<usize, usize> = HashMap::new();
    let mut pieces = pieces.into_iter();
    for (k, node) in nodes.iter().enumerate() {
        let key = owner[k].unwrap_or(k);
        let g = *at_owner.entry(key).or_insert_with(|| {
            groups.push((
                node.slot,
                Warehouse {
                    entrance: None,
                    modules: Vec::new(),
                },
            ));
            groups.len() - 1
        });
        let w = &mut groups[g].1;
        if k < n_entrances {
            w.entrance = Some((*node).clone());
        } else {
            w.modules.push(Module {
                centre: centres[k - n_entrances],
                class: class(&node.model).expect("a centre piece has a class"),
                pieces: pieces.next().unwrap_or_default(),
            });
        }
        groups[g].0 = groups[g].0.min(node.slot);
    }
    groups.retain(|(_, w)| !w.modules.is_empty());
    groups.sort_by_key(|(slot, _)| *slot);
    groups.into_iter().map(|(_, w)| w).collect()
}

/// The marks of one module: the joined outline of its boxes as open runs,
/// its other boxes and lines as runs and loops, its tanks as circles
/// (centre and radius) and its dashed boxes as loops.
#[derive(Default)]
struct Marks {
    runs: Vec<Vec<(f64, f64)>>,
    loops: Vec<Vec<(f64, f64)>>,
    circles: Vec<((f64, f64), f64)>,
    dashed: Vec<Vec<(f64, f64)>>,
}

fn module_marks(
    picture: &Picture,
    module: &Module,
    models: &HashMap<String, WarehouseModel>,
) -> Marks {
    let mut out = Marks::default();
    let mut boxes = Vec::new();
    for i in &module.pieces {
        let (Some(m), Some(how)) = (models.get(&i.model), mark(&i.model)) else {
            continue;
        };
        let (a0, b0, a1, b1) = m.bounds;
        let at = |a: f64, b: f64| picture.pic(i.place(a, b));
        match how {
            Mark::Box => boxes.push(corners(picture, i, m.bounds)),
            Mark::Bays => {
                for &bay in &m.bays {
                    out.loops.push(corners(picture, i, bay));
                    out.runs.extend(
                        hatch(bay)
                            .into_iter()
                            .map(|((a0, b0), (a1, b1))| vec![at(a0, b0), at(a1, b1)]),
                    );
                }
            }
            Mark::Tank => {
                if let Some(&(c0, d0, c1, d1)) = m.bays.first() {
                    let r = (c1 - c0).min(d1 - d0) / 2.0;
                    out.circles.push((at((c0 + c1) / 2.0, (d0 + d1) / 2.0), r));
                }
            }
            Mark::Pipe => out.runs.push(if a1 - a0 >= b1 - b0 {
                let b = (b0 + b1) / 2.0;
                vec![at(a0, b), at(a1, b)]
            } else {
                let a = (a0 + a1) / 2.0;
                vec![at(a, b0), at(a, b1)]
            }),
            Mark::Rails => out.runs.extend(if a1 - a0 <= b1 - b0 {
                [vec![at(a0, b0), at(a0, b1)], vec![at(a1, b0), at(a1, b1)]]
            } else {
                [vec![at(a0, b0), at(a1, b0)], vec![at(a0, b1), at(a1, b1)]]
            }),
            Mark::Yard => out.dashed.push(corners(picture, i, m.bounds)),
        }
    }
    if !boxes.is_empty() {
        out.runs.extend(outer_runs(&boxes));
    }
    out
}

/// Lines at 45 degrees across a box, `HATCH_STEP` apart along its sides,
/// none through a corner: (from, to) in the box's axes.
fn hatch((a0, b0, a1, b1): Rect) -> Vec<((f64, f64), (f64, f64))> {
    // Each line is a - b = t, for t across the box's range of a - b.
    let (t0, t1) = (a0 - b1, a1 - b0);
    let n = ((t1 - t0) / HATCH_STEP).floor() as usize;
    let margin = (t1 - t0 - n as f64 * HATCH_STEP) / 2.0;
    (0..=n)
        .map(|k| t0 + margin + k as f64 * HATCH_STEP)
        .filter(|&t| t > t0 + 0.1 && t < t1 - 0.1)
        .map(|t| {
            let (lo, hi) = (a0.max(b0 + t), a1.min(b1 + t));
            ((lo, lo - t), (hi, hi - t))
        })
        .collect()
}

/// The data attributes of an instance.
fn data(i: &Instance) -> String {
    format!(
        "data-slot=\"{}\" data-model=\"{}\" data-x=\"{}\" data-y=\"{}\"",
        i.slot,
        escape(&i.model),
        fmt1(i.x),
        fmt1(i.y)
    )
}

/// The lines of the `warehouses` layer: each warehouse among `rows` that
/// `cut` meets, worked out from all of `rows`.
pub(super) fn layers(
    picture: &Picture,
    rows: &[Instance],
    models: &HashMap<String, WarehouseModel>,
    metres_per_pixel: f64,
    cut: Cut,
) -> Vec<String> {
    let width = (DETAIL_PX * metres_per_pixel).min(OUTLINE / 2.0);
    let mut out = Vec::new();
    for w in warehouses(rows) {
        let faces: Vec<Vec<(f64, f64)>> = w
            .entrance
            .iter()
            .chain(w.modules.iter().map(|m| m.centre))
            .map(|i| corners(picture, i, face(i)))
            .collect();
        let all: Vec<(f64, f64)> = faces.iter().flatten().copied().collect();
        if !cut.meets(&all, OUTLINE / 2.0) {
            continue;
        }
        if out.is_empty() {
            out.push(layer(
                "warehouses",
                "Warehouses",
                " pointer-events=\"none\"",
            ));
        }
        let ground = loops_d(&closed_outline(&faces, GROUND_CLOSE));
        let first = w.entrance.as_ref().unwrap_or(w.modules[0].centre);
        out.push(format!(
            "<g class=\"warehouse\" {} data-modules=\"{}\">",
            data(first),
            w.modules.len()
        ));
        out.push(format!(
            "<path class=\"warehouse-ground\" d=\"{ground}\" fill=\"{GROUND_FILL}\" fill-rule=\"evenodd\" \
             stroke=\"{GROUND_FILL}\" stroke-width=\"0.5\"/>"
        ));
        // Every warehouse has a module.
        out.push(format!(
            "<g class=\"warehouse-marks\" fill=\"none\" stroke=\"{INK}\" stroke-width=\"{}\" \
                 stroke-linecap=\"round\" stroke-linejoin=\"round\" opacity=\"{INK_OPACITY}\">",
            fmt_g(width)
        ));
        for m in &w.modules {
            let marks = module_marks(picture, m, models);
            let mut line = format!(
                "<g class=\"warehouse-module\" data-type=\"{}\" {}>",
                m.class.name(),
                data(m.centre)
            );
            let mut d = runs_d(&marks.runs);
            d.push_str(&loops_d(&marks.loops));
            if !d.is_empty() {
                line += &format!("<path d=\"{d}\"/>");
            }
            for ((cx, cy), r) in &marks.circles {
                line += &format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
                    fmt2(*cx),
                    fmt2(*cy),
                    fmt2(*r)
                );
            }
            if !marks.dashed.is_empty() {
                line += &format!(
                    "<path d=\"{}\" stroke-dasharray=\"{} {}\"/>",
                    loops_d(&marks.dashed),
                    fmt_g(4.0 * width),
                    fmt_g(3.0 * width)
                );
            }
            line += "</g>";
            out.push(line);
        }
        out.push("</g>".into());
        out.push(format!(
            "<path class=\"warehouse-outer\" d=\"{ground}\" fill=\"none\" stroke=\"#000\" \
             stroke-width=\"{OUTLINE}\" stroke-linejoin=\"round\"/>"
        ));
        out.push("</g>".into());
    }
    if !out.is_empty() {
        out.push("</g>".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::svg::North;

    fn piece(slot: usize, model: &str, at: (f64, f64), facing: (f64, f64)) -> Instance {
        Instance {
            slot,
            model: model.into(),
            x: at.0,
            y: at.1,
            z: 0.0,
            fx: facing.0,
            fy: facing.1,
        }
    }

    fn picture() -> Picture {
        Picture {
            width: 1000.0,
            height: 1000.0,
            north: North::Game,
        }
    }

    /// An entrance at (0, -3) of a construction at the origin, a goods
    /// module 24 m beyond the origin, a liquid one 32 m beyond that turned
    /// a half turn (as the script turns alternate ones), and a lone bulk
    /// module far off.
    fn rows() -> Vec<Instance> {
        vec![
            piece(10, ENTRANCE, (0.0, -3.0), (1.0, 0.0)),
            piece(
                11,
                "warehouses/wh_goods/top_c_right.mdl",
                (0.0, 24.0),
                (1.0, 0.0),
            ),
            piece(
                12,
                "warehouses/wh_goods/top_c_left.mdl",
                (0.0, 24.0),
                (1.0, 0.0),
            ),
            piece(
                13,
                "warehouses/wh_goods/bot_right.mdl",
                (0.0, 24.0),
                (1.0, 0.0),
            ),
            piece(
                14,
                "warehouses/wh_goods/bot_left.mdl",
                (0.0, 24.0),
                (1.0, 0.0),
            ),
            piece(
                15,
                "warehouses/wh_liquid/wh_liquid_con_1.mdl",
                (0.0, 56.0),
                (1.0, 0.0),
            ),
            piece(
                16,
                "warehouses/wh_liquid/wh_liquid.mdl",
                (0.0, 56.0),
                (-1.0, 0.0),
            ),
            piece(
                17,
                "warehouses/wh_liquid/wh_liquid_roof.mdl",
                (0.0, 56.0),
                (-1.0, 0.0),
            ),
            piece(
                20,
                "warehouses/wh_bulk/wh_bulk_center.mdl",
                (300.0, 0.0),
                (0.0, 1.0),
            ),
        ]
    }

    fn models() -> HashMap<String, WarehouseModel> {
        let model = |bounds, bays| WarehouseModel { bounds, bays };
        HashMap::from([
            (
                "warehouses/wh_goods/top_c_right.mdl".to_owned(),
                model((-0.5, 0.0, 12.0, 16.0), vec![]),
            ),
            (
                "warehouses/wh_goods/top_c_left.mdl".to_owned(),
                model((-12.0, 0.0, -0.5, 16.0), vec![]),
            ),
            (
                "warehouses/wh_goods/bot_right.mdl".to_owned(),
                model((-0.5, -11.0, 12.0, 0.5), vec![]),
            ),
            (
                "warehouses/wh_goods/bot_left.mdl".to_owned(),
                model((-12.0, -11.0, -0.5, 0.5), vec![]),
            ),
            (
                "warehouses/wh_liquid/wh_liquid.mdl".to_owned(),
                model((-14.0, -14.0, 14.0, 14.0), vec![(-9.5, -9.5, 9.5, 9.5)]),
            ),
            (
                "warehouses/wh_liquid/wh_liquid_con_1.mdl".to_owned(),
                model((-1.0, -16.0, 1.0, -10.0), vec![]),
            ),
            (
                "warehouses/wh_bulk/wh_bulk_center.mdl".to_owned(),
                model(
                    (-13.0, -15.0, 13.0, 15.0),
                    vec![(0.0, -12.0, 11.0, 10.0), (-11.0, -12.0, 0.0, 10.0)],
                ),
            ),
        ])
    }

    #[test]
    fn modules_join_their_entrance_and_keep_their_class() {
        let rows = rows();
        let found = warehouses(&rows);
        assert_eq!(found.len(), 2);
        let first = &found[0];
        assert_eq!(first.entrance.as_ref().map(|e| e.slot), Some(10));
        let classes: Vec<(usize, Class, usize)> = first
            .modules
            .iter()
            .map(|m| (m.centre.slot, m.class, m.pieces.len()))
            .collect();
        // The roof has no mark, so it is no piece of its module.
        assert_eq!(classes, [(11, Class::Goods, 4), (16, Class::Liquid, 2)]);
        assert!(found[1].entrance.is_none());
        assert_eq!(found[1].modules[0].class, Class::Bulk);
    }

    #[test]
    fn an_entrance_with_no_turn_takes_its_modules() {
        // The first module lies 27 m towards map -x of the entrance, so the
        // entrance's +y is (-1, 0) and its x axis (0, 1), as the module's is.
        let rows = vec![
            piece(1, ENTRANCE, (100.0, 0.0), (0.0, 0.0)),
            piece(
                2,
                "warehouses/wh_flatbed/wh_flatbed_rails.mdl",
                (73.0, 0.0),
                (0.0, 1.0),
            ),
            // 48 m on along the entrance's x: its face does not meet the
            // first module's, but it stands on the entrance's grid.
            piece(
                3,
                "warehouses/wh_goods/top_right.mdl",
                (73.0, 48.0),
                (0.0, 1.0),
            ),
            // Off the grid by 4 m: another warehouse's.
            piece(
                4,
                "warehouses/wh_goods/top_right.mdl",
                (73.0, -52.0),
                (0.0, 1.0),
            ),
        ];
        let found = warehouses(&rows);
        assert_eq!(found.len(), 2);
        let slots: Vec<usize> = found[0].modules.iter().map(|m| m.centre.slot).collect();
        assert_eq!(slots, [2, 3]);
        assert!(found[1].entrance.is_none());
        let e = found[0].entrance.as_ref().unwrap();
        assert_eq!((e.slot, e.x, e.fx, e.fy), (1, 100.0, 0.0, 1.0));
        // The entrance's face reaches 15 m towards the module, 9 m away from it.
        let (a0, b0, a1, b1) = face(e);
        let xs: Vec<f64> = [(a0, b0), (a1, b1)]
            .iter()
            .map(|&(a, b)| e.place(a, b).0)
            .collect();
        assert_eq!(xs, [109.0, 85.0]);
    }

    #[test]
    fn warehouses_built_against_each_other_stay_two() {
        let goods = "warehouses/wh_goods/top_right.mdl";
        let rows = vec![
            // Side by side: entrance faces 40 m wide, 40 m apart.
            piece(1, ENTRANCE, (0.0, -3.0), (1.0, 0.0)),
            piece(2, ENTRANCE, (40.0, -3.0), (1.0, 0.0)),
            piece(3, goods, (0.0, 24.0), (1.0, 0.0)),
            piece(4, goods, (40.0, 24.0), (1.0, 0.0)),
            // Back to back: origins 24 m apart, entrances facing away.
            piece(5, ENTRANCE, (500.0, 3.0), (1.0, 0.0)),
            piece(6, ENTRANCE, (500.0, -21.0), (-1.0, 0.0)),
            piece(7, goods, (500.0, 30.0), (1.0, 0.0)),
            piece(8, goods, (500.0, -48.0), (-1.0, 0.0)),
        ];
        let found: Vec<(usize, Vec<usize>)> = warehouses(&rows)
            .iter()
            .map(|w| {
                let e = w.entrance.as_ref().unwrap().slot;
                (e, w.modules.iter().map(|m| m.centre.slot).collect())
            })
            .collect();
        assert_eq!(
            found,
            [(1, vec![3]), (2, vec![4]), (5, vec![7]), (6, vec![8])]
        );
    }

    #[test]
    fn pieces_with_no_unit_turn_and_lone_entrances_are_left_out() {
        // As in the preview block's tilted pages: a centre piece turned 0.73, another not
        // at all, and an entrance with none of its own and no module near.
        let rows = vec![
            piece(
                1,
                "warehouses/wh_bulk/wh_bulk_center.mdl",
                (15.6, -2.2),
                (0.73, 0.0),
            ),
            piece(
                2,
                "warehouses/wh_liquid/wh_liquid.mdl",
                (15.6, 7.2),
                (0.0, 0.0),
            ),
            piece(
                3,
                "warehouses/wh_liquid/wh_liquid_con_2.mdl",
                (15.6, 7.2),
                (1.0, 0.0),
            ),
            piece(4, ENTRANCE, (140.5, 7.2), (0.74, 0.0)),
            piece(5, ENTRANCE, (500.0, 0.0), (1.0, 0.0)),
        ];
        assert!(warehouses(&rows).is_empty());
        assert_eq!(count(&rows), (0, 0));
    }

    #[test]
    fn faces_turned_apart_do_not_meet() {
        let a = piece(
            1,
            "warehouses/wh_bulk/wh_bulk_center.mdl",
            (0.0, 0.0),
            (1.0, 0.0),
        );
        let b = piece(
            2,
            "warehouses/wh_bulk/wh_bulk_center.mdl",
            (32.0, 0.0),
            (0.6, 0.8),
        );
        let c = piece(
            3,
            "warehouses/wh_bulk/wh_bulk_center.mdl",
            (32.0, 8.0),
            (-1.0, 0.0),
        );
        let d = piece(
            4,
            "warehouses/wh_bulk/wh_bulk_center.mdl",
            (41.0, 0.0),
            (1.0, 0.0),
        );
        assert!(!faces_meet(&a, &b));
        // Half a turn aside, 8 m along the edge: one warehouse.
        assert!(faces_meet(&a, &c));
        // A metre apart: two.
        assert!(!faces_meet(&a, &d));
    }

    #[test]
    fn each_module_is_marked_by_its_class() {
        let text = layers(&picture(), &rows(), &models(), 1.0, Cut(None)).join("\n");
        assert_eq!(text.matches("class=\"warehouse\"").count(), 2);
        assert!(text.contains(
            "<g class=\"warehouse\" data-slot=\"10\" data-model=\"warehouses/entrance/entrance_sm.mdl\" \
             data-x=\"0.0\" data-y=\"-3.0\" data-modules=\"2\">"
        ));
        assert!(text.contains("data-type=\"goods\" data-slot=\"11\""));
        assert!(text.contains("data-type=\"liquid\" data-slot=\"16\""));
        assert!(text.contains("data-type=\"bulk\" data-slot=\"20\""));
        // The tank, round the liquid module's centre (picture v = 500 - 56).
        assert!(
            text.contains("<circle cx=\"500.00\" cy=\"444.00\" r=\"9.50\"/>"),
            "{text}"
        );
        // Half a pixel at 1 m per pixel; never more than half the outline.
        assert!(text.contains("stroke-width=\"0.5\""));
        let coarse = layers(&picture(), &rows(), &models(), 8.0, Cut(None)).join("\n");
        assert!(coarse.contains("stroke-width=\"0.75\""));
        // The goods hall is one outline: no line where its corners meet, at
        // the module's centre (500, 476).
        let goods = text
            .lines()
            .find(|l| l.contains("data-type=\"goods\""))
            .unwrap();
        assert!(!goods.contains("500.00 476.00"), "{goods}");
        assert!(goods.contains("<path d=\"M"), "{goods}");
    }

    #[test]
    fn the_ground_is_one_shape_round_the_entrance_and_the_modules() {
        let text = layers(&picture(), &rows(), &HashMap::new(), 4.0, Cut(None)).join("\n");
        let ground = text
            .lines()
            .find(|l| l.contains("class=\"warehouse-ground\""))
            .unwrap();
        // One loop: from the entrance face's near edge (v = 512) to the far
        // edge of the liquid square (v = 424).
        assert_eq!(ground.matches('M').count(), 1, "{ground}");
        assert!(
            ground.contains(" 512.00") && ground.contains(" 424.00"),
            "{ground}"
        );
        // Without the models the modules are there with no marks.
        assert!(text.contains("data-type=\"goods\" data-slot=\"11\" data-model=\"warehouses/wh_goods/top_c_right.mdl\" data-x=\"0.0\" data-y=\"24.0\"></g>"));
        assert!(!text.contains("<circle"));
    }

    #[test]
    fn a_hatch_line_runs_from_side_to_side() {
        let lines = hatch((0.0, 0.0, 10.0, 20.0));
        // a - b runs from -20 to 10: 8 lines 4 m apart, 1 m in from each end.
        assert_eq!(lines.len(), 8);
        assert_eq!(lines[0], ((0.0, 19.0), (1.0, 20.0)));
        assert_eq!(lines[7], ((9.0, 0.0), (10.0, 1.0)));
        for ((a0, b0), (a1, b1)) in lines {
            assert!((a1 - a0 - (b1 - b0)).abs() < 1e-9);
            for (a, b) in [(a0, b0), (a1, b1)] {
                assert!((0.0..=10.0).contains(&a) && (0.0..=20.0).contains(&b));
            }
        }
    }

    #[test]
    fn a_crop_writes_only_the_warehouses_it_meets() {
        let cut = Cut(Some((780.0, 820.0, 480.0, 520.0)));
        let text = layers(&picture(), &rows(), &models(), 1.0, cut).join("\n");
        assert_eq!(text.matches("class=\"warehouse\"").count(), 1);
        assert!(text.contains("data-type=\"bulk\""));
        let none = layers(
            &picture(),
            &rows(),
            &models(),
            1.0,
            Cut(Some((0.0, 10.0, 0.0, 10.0))),
        );
        assert!(none.is_empty());
    }
}
