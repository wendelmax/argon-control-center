#!/bin/sh
# Assembles the Plasma widgets into DEST: each widget gets the shared components
# from common/, and the style configuration when it has a .styles marker.
#   packaging/plasma/stage.sh <DEST>   -> DEST/org.argon.controlcenter.*
set -eu
DEST="$1"
HERE="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$DEST"
for widget in "$HERE"/org.argon.controlcenter.*; do
    name="$(basename "$widget")"
    out="$DEST/$name"
    rm -rf "$out"
    mkdir -p "$out/contents/ui"
    cp "$widget/metadata.json" "$out/"
    cp "$widget"/contents/ui/*.qml "$out/contents/ui/"
    cp "$HERE"/common/ArgonData.qml "$HERE"/common/Meter.qml "$HERE"/common/Gauge.qml "$HERE"/common/Spark.qml "$HERE"/common/GaugeGrid.qml "$out/contents/ui/"
    if [ -f "$widget/.styles" ]; then
        mkdir -p "$out/contents/config"
        cp "$HERE/common/config.qml" "$HERE/common/main.xml" "$out/contents/config/"
        cp "$HERE/common/configGeneral.qml" "$out/contents/ui/"
    fi
done
