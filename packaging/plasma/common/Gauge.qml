import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

// Radial gauge (270° arc) like the app's gauges view. The arc colour follows
// the value: green -> amber -> red.
ColumnLayout {
    id: g
    property real fraction: 0
    property string value: ""
    property string unit: ""
    property string label: ""
    property real size: Kirigami.Units.gridUnit * 5
    spacing: 0

    function levelColor(f) {
        return f < 0.6 ? "#27ae60" : (f < 0.85 ? "#f39c12" : "#da4453")
    }

    PlasmaComponents.Label {
        text: g.label
        font.bold: true
        Layout.alignment: Qt.AlignHCenter
    }
    Item {
        Layout.preferredWidth: g.size
        Layout.preferredHeight: g.size * 0.85
        Layout.alignment: Qt.AlignHCenter
        Canvas {
            id: canvas
            anchors.fill: parent
            property real f: Math.max(0, Math.min(1, g.fraction))
            onFChanged: requestPaint()
            onWidthChanged: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                var r = Math.min(width, height / 0.85) * 0.4
                var cx = width / 2, cy = height * 0.55
                var start = 0.75 * Math.PI, sweep = 1.5 * Math.PI
                ctx.lineWidth = Math.max(4, r * 0.16)
                ctx.lineCap = "round"
                ctx.strokeStyle = Qt.rgba(0.5, 0.5, 0.5, 0.25)
                ctx.beginPath(); ctx.arc(cx, cy, r, start, start + sweep); ctx.stroke()
                if (f > 0.005) {
                    ctx.strokeStyle = g.levelColor(f)
                    ctx.beginPath(); ctx.arc(cx, cy, r, start, start + sweep * f); ctx.stroke()
                }
            }
        }
        ColumnLayout {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: g.size * 0.04
            spacing: 0
            PlasmaComponents.Label {
                text: g.value
                font.pixelSize: g.size * 0.2
                font.bold: true
                Layout.alignment: Qt.AlignHCenter
            }
            PlasmaComponents.Label {
                text: g.unit
                opacity: 0.7
                Layout.alignment: Qt.AlignHCenter
            }
        }
    }
}
