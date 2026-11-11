import QtQuick

// Lays out radial gauges in the column count that gives the largest gauges for
// the available space, so the widget can be resized freely.
// items: [{ label, fraction, value, unit }]
Item {
    id: grid
    property var items: []
    signal activated()
    readonly property int count: items ? items.length : 0
    // Gauge cell is about 1.15x taller than wide (label above the arc).
    readonly property real aspect: 1.15
    readonly property int columns: {
        var best = 1, bestSize = 0
        for (var c = 1; c <= Math.max(1, count); c++) {
            var rows = Math.ceil(count / c)
            var size = Math.min(width / c, height / (rows * aspect))
            if (size > bestSize) { bestSize = size; best = c }
        }
        return best
    }
    readonly property real cell: count > 0
        ? Math.max(0, Math.min(width / columns, height / (Math.ceil(count / columns) * aspect)))
        : 0

    Grid {
        anchors.centerIn: parent
        columns: grid.columns
        Repeater {
            model: grid.items
            Gauge {
                size: grid.cell * 0.94
                label: modelData.label
                fraction: modelData.fraction
                value: modelData.value
                unit: modelData.unit
            }
        }
    }
    MouseArea {
        anchors.fill: parent
        onClicked: grid.activated()
    }
}
