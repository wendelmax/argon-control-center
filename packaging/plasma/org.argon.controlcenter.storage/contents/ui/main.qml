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

    toolTipMainText: "Argon Storage"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    // Gauges float on the desktop without the widget frame.
    Plasmoid.backgroundHints: root.style === "gauges" ? PlasmaCore.Types.NoBackground : PlasmaCore.Types.DefaultBackground
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    function record() {}

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready && root.s.disks.length > 0 ? "/ " + data.pct(root.s.disks[0].usage_pct) : "—"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; visible: root.style !== "gauges"; text: "Storage" }
        GaugeGrid {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: Kirigami.Units.gridUnit * 4
            Layout.preferredHeight: Kirigami.Units.gridUnit * 8
            items: (root.ready ? root.s.disks.map(function (d) {
                return { label: d.mount_point === "/" ? "/" : d.mount_point.split("/").pop(), fraction: d.usage_pct / 100, value: Math.round(d.usage_pct), unit: "%" } }) : [])
            onActivated: data.openApp()
        }
        ColumnLayout {
            visible: root.style !== "gauges"
            Layout.fillWidth: true
            Repeater {
                model: root.ready ? root.s.disks : []
                Meter {
                    Layout.fillWidth: true
                    label: modelData.mount_point === "/" ? "/" : modelData.mount_point.split("/").pop()
                    fraction: modelData.usage_pct / 100
                    value: modelData.used_gb.toFixed(0) + "/" + modelData.total_gb.toFixed(0) + "G"
                }
            }
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
