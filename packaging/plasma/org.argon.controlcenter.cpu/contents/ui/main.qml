import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root
    readonly property var s: data.snap
    readonly property bool ready: data.ready
    readonly property string style: Plasmoid.configuration.style !== undefined ? Plasmoid.configuration.style : "bars"
    readonly property var palette: ["#3daee9", "#27ae60", "#f67400", "#9b59b6", "#da4453", "#1abc9c", "#f39c12", "#95a5a6"]

    ArgonData { id: data; onUpdated: root.record() }

    toolTipMainText: "Argon CPU"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    property var hTotal: []
    property var hCores: []
    function record() {
        hTotal = data.push(hTotal, s.cpu.total)
        var next = []
        for (var i = 0; i < s.cpu.cores.length; i++) next.push(data.push(hCores[i] || [], s.cpu.cores[i]))
        hCores = next
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready ? "CPU " + data.pct(root.s.cpu.total) : "CPU —"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "CPU" }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "total"
                fraction: root.ready ? root.s.cpu.total / 100 : 0
                value: root.ready ? data.pct(root.s.cpu.total) : "—"
            }
        }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Repeater {
                model: root.ready ? root.s.cpu.cores : []
                Meter {
                    Layout.fillWidth: true
                    label: "cpu" + index
                    fraction: modelData / 100
                    value: data.pct(modelData)
                }
            }
        }
        Flow {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Gauge {
                visible: true
                label: "total"
                fraction: root.ready ? root.s.cpu.total / 100 : 0
                value: root.ready ? Math.round(root.s.cpu.total) : "—"
                unit: "%"
            }
        }
        Flow {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Repeater {
                model: root.ready ? root.s.cpu.cores : []
                Gauge {
                    size: Kirigami.Units.gridUnit * 3.6
                    label: "cpu" + index
                    fraction: modelData / 100
                    value: Math.round(modelData)
                    unit: "%"
                }
            }
        }
        Spark {
            visible: root.style === "chart"
            series: [{ values: root.hTotal, color: Kirigami.Theme.textColor, max: 100 }].concat(root.hCores.map(function (h, i) { return { values: h, color: root.palette[i % root.palette.length], max: 100 } }))
        }
        PlasmaComponents.Label {
            text: root.ready ? root.s.cpu.mhz + " MHz · " + root.s.cpu.governor : ""
            opacity: 0.7
        }
        PlasmaComponents.ToolButton {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: data.openApp()
        }
    }
}
