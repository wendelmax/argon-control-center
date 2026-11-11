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

    toolTipMainText: "Argon System"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    function record() {}

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready ? "up " + data.uptime(root.s.system.uptime_s) : "—"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "System" }
        GridLayout {
            columns: 2
            columnSpacing: Kirigami.Units.largeSpacing
            PlasmaComponents.Label { text: "board"; opacity: 0.7 }
            PlasmaComponents.Label { text: root.ready ? root.s.board : "—"; Layout.fillWidth: true; elide: Text.ElideRight }
            PlasmaComponents.Label { text: "os"; opacity: 0.7 }
            PlasmaComponents.Label { text: root.ready ? root.s.system.os : "—"; Layout.fillWidth: true; elide: Text.ElideRight }
            PlasmaComponents.Label { text: "kernel"; opacity: 0.7 }
            PlasmaComponents.Label { text: root.ready ? root.s.system.kernel : "—"; Layout.fillWidth: true; elide: Text.ElideRight }
            PlasmaComponents.Label { text: "uptime"; opacity: 0.7 }
            PlasmaComponents.Label { text: root.ready ? data.uptime(root.s.system.uptime_s) : "—" }
            PlasmaComponents.Label { text: "load"; opacity: 0.7 }
            PlasmaComponents.Label { text: root.ready ? root.s.system.load.map(function (v) { return v.toFixed(2) }).join("  ") : "—" }
        }
        PlasmaComponents.ToolButton {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: data.openApp()
        }
    }
}
