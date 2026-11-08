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
# mod.io 6433459: a wide road in a tunnel under a warehouse.
draw 6433459.sav warehouse-tunnel.svg --crop -1482,1231,480 --mpp 1
