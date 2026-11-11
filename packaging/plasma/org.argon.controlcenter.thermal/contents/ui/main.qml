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

    toolTipMainText: "Argon Thermal"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    property var hTemp: []
    property var hFan: []
    function record() {
        hTemp = data.push(hTemp, s.temp_c)
        hFan = data.push(hFan, s.fan_pct)
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready ? root.s.temp_display + " · " + root.s.fan_pct + "%" : "—"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "Thermal" }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "temp"
                fraction: root.ready ? (root.s.temp_c - 25) / 60 : 0
                value: root.ready ? root.s.temp_display : "—"
            }
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "fan"
                fraction: root.ready ? root.s.fan_pct / 100 : 0
                value: root.ready ? root.s.fan_pct + "%" : "—"
            }
        }
        Flow {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Gauge {
                visible: true
                label: "temp"
                fraction: root.ready ? (root.s.temp_c - 25) / 60 : 0
                value: root.ready ? root.s.temp_c.toFixed(1) : "—"
                unit: "°C"
            }
            Gauge {
                visible: true
                label: "fan"
                fraction: root.ready ? root.s.fan_pct / 100 : 0
                value: root.ready ? root.s.fan_pct : "—"
                unit: "%"
            }
        }
        Spark {
            visible: root.style === "chart"
            series: [{ values: root.hTemp, color: "#f67400", max: 90 }, { values: root.hFan, color: "#27ae60", max: 100 }]
        }
        PlasmaComponents.ToolButton {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: data.openApp()
        }
    }
}
