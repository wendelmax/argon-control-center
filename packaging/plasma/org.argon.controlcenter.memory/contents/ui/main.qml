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

    toolTipMainText: "Argon Memory"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    // Gauges float on the desktop without the widget frame.
    Plasmoid.backgroundHints: root.style === "gauges" ? PlasmaCore.Types.NoBackground : PlasmaCore.Types.DefaultBackground
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    property var hMem: []
    property var hSwap: []
    function record() {
        hMem = data.push(hMem, s.mem.pct)
        hSwap = data.push(hSwap, s.swap.pct)
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready ? "RAM " + data.pct(root.s.mem.pct) : "RAM —"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; visible: root.style !== "gauges"; text: "Memory" }
        GaugeGrid {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: Kirigami.Units.gridUnit * 4
            Layout.preferredHeight: Kirigami.Units.gridUnit * 8
            items: (root.ready ? [
                { label: "mem", fraction: root.s.mem.pct / 100, value: Math.round(root.s.mem.pct), unit: "%" },
                { label: "swap", fraction: root.s.swap.pct / 100, value: Math.round(root.s.swap.pct), unit: "%" }] : [])
            onActivated: data.openApp()
        }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "mem"
                fraction: root.ready ? root.s.mem.pct / 100 : 0
                value: root.ready ? (root.s.mem.used_mb / 1024).toFixed(1) + "/" + (root.s.mem.total_mb / 1024).toFixed(1) + "G" : "—"
            }
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "swap"
                fraction: root.ready ? root.s.swap.pct / 100 : 0
                value: root.ready ? root.s.swap.used_mb + "/" + root.s.swap.total_mb + "M" : "—"
            }
        }
        Spark {
            visible: root.style === "chart"
            series: [{ values: root.hMem, color: "#9b59b6", max: 100 }, { values: root.hSwap, color: "#f39c12", max: 100 }]
        }
        PlasmaComponents.ToolButton {
            visible: root.style !== "gauges"
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: data.openApp()
        }
    }
}
