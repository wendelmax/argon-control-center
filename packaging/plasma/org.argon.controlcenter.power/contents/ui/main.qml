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

    toolTipMainText: "Argon Power"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    function record() {}

    compactRepresentation: MouseArea {
        Layout.minimumWidth: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        Rectangle {
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.small * 0.7
            height: width
            radius: width / 2
            color: !root.ready ? Kirigami.Theme.disabledTextColor
                 : (root.s.power.problem || !root.s.daemon_active || root.s.fan_bus === false) ? Kirigami.Theme.negativeTextColor
                 : Kirigami.Theme.positiveTextColor
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "Power" }
        PlasmaComponents.Label {
            text: "5V: " + (root.ready && root.s.power.undervoltage_now ? "UNDER-VOLTAGE" : "OK")
            color: root.ready && root.s.power.undervoltage_now ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.positiveTextColor
        }
        PlasmaComponents.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            visible: root.ready && root.s.power.throttled !== null
            text: visible ? "Firmware: " + root.s.power.summary + "  (" + root.s.power.throttled + ")" : ""
            color: root.ready && root.s.power.problem ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
        }
        PlasmaComponents.Label {
            visible: root.ready && root.s.fan_bus === false
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: "Fan: I2C off, the fan is not being controlled"
            color: Kirigami.Theme.negativeTextColor
        }
        PlasmaComponents.Label {
            text: "argononed: " + (root.ready && root.s.daemon_active ? "active" : "stopped")
            color: root.ready && root.s.daemon_active ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
        }
        PlasmaComponents.ToolButton {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "argon-control-center"
            onClicked: data.openApp()
        }
    }
}
