#!/bin/sh
# Redraw the example maps in docs/images/ with the current build, so they
# can be redrawn as the drawing improves. The saves are not in the repo: each
# is kept under local/exemplar-saves/ on the maintainer's machine, and a map
# whose save is missing is skipped.
set -eu
cd "$(dirname "$0")/.."
cargo build --release -q
TF3MAP=target/release/tf3map
SAVES=local/exemplar-saves

draw() {
    save=$1 out=$2
    shift 2
    if [ -f "$SAVES/$save" ]; then
        "$TF3MAP" "$SAVES/$save" "$@" -o "docs/images/$out"
    else
        echo "skipped $out: no $SAVES/$save" >&2
    fi
}

# The maintainer's own save: an oil refinery and its harbour, the rail bridge
# and ship depot to the west, the freight yard, its loading platforms and the
# rail depot to the east.
draw "WR26A Giga-Tv3 1919 1107-1558.sav" peterlee-refinery.svg \
    --crop 8460,-3320,1050 --mpp 2
# The maintainer's own editor map: two small towns and the logging camp
# between them, named by hand with --text (the save does not say where its
# towns are).
draw "WR26B Giga Dry v1 1108-1359 generate main connections.sav" texts-deming-quemado.svg \
    --crop 2300,-8000,4000 --text "-410,-7671=Deming" --text "5163,-9172=Quemado" \
    --text "2019,-9340,small=Deming Logging Camp"
# mod.io 6433459: a wide road in a tunnel under a warehouse.
draw 6433459.sav warehouse-tunnel.svg --crop -1482,1231,480 --mpp 1

# Rail grades (docs/rail-grades.md), all from saves published in the mod.io
# catalogue.
# mod.io 6414627: a helix of six rings, centred, with the line out of it.
draw 6414627.sav rail-grades-helix.svg --crop 4680,-1410,940 --mpp 1 --rail-grades
# mod.io 6459692: a four-track mountain line round a horseshoe curve, with
# tunnels at both ends.
draw 6459692.sav rail-grades-horseshoe.svg --crop -1500,0,940 --mpp 1 --rail-grades
# mod.io 6487589: a busy junction, tunnels under the roads and a many-track
# bridge.
draw 6487589.sav rail-grades-junction.svg --crop -1750,-5250,500 --mpp 1 --rail-grades
# mod.io 6433417: a fan of seven lines out of one bundle.
draw 6433417.sav rail-grades-fan.svg --crop 950,700,940 --mpp 1 --rail-grades
# mod.io 6448903: a harbour town, centred, with a multi-track line and its
# tunnel down the west side.
draw 6448903.sav rail-grades-town.svg --crop 0,-600,1000 --mpp 1 --rail-grades
