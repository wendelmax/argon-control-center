import QtQuick
import QtQuick.Layouts
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.plasma5support as P5Support
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root
    property var snap: null
    readonly property bool ready: snap !== null
    // Per-user installs live in ~/.local/bin, which is often not in plasmashell's PATH.
    readonly property string cmd: "PATH=\"$PATH:$HOME/.local/bin\" argon-control-center"

    // Reads `argon-control-center --json` every 2 s.
    P5Support.DataSource {
        engine: "executable"
        connectedSources: [root.cmd + " --json"]
        interval: 2000
        onNewData: (source, data) => {
            try { root.snap = JSON.parse(data["stdout"]) } catch (e) { root.snap = null }
        }
    }
    // Opens the application.
    P5Support.DataSource {
        id: launcher
        engine: "executable"
        onNewData: (source) => disconnectSource(source)
    }
    function openApp() {
        launcher.connectSource(root.cmd + " --gui >/dev/null 2>&1 &")
    }
    function pct(v) { return (v === null || v === undefined) ? "—" : Math.round(v) + "%" }

    toolTipMainText: "Argon Control Center"
    toolTipSubText: ready ? snap.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation

    compactRepresentation: MouseArea {
        Layout.minimumWidth: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        Rectangle {
            anchors.centerIn: parent
            width: Kirigami.Units.iconSizes.small * 0.7
            height: width
            radius: width / 2
            color: !root.ready ? Kirigami.Theme.disabledTextColor
                 : (root.snap.power.problem || !root.snap.daemon_active) ? Kirigami.Theme.negativeTextColor
                 : Kirigami.Theme.positiveTextColor
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "Power" }
        PlasmaComponents.Label {
            text: "5V: " + (root.ready && root.snap.power.undervoltage_now ? "UNDER-VOLTAGE" : "OK")
            color: root.ready && root.snap.power.undervoltage_now ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.positiveTextColor
        }
        PlasmaComponents.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            visible: root.ready && root.snap.power.throttled !== null
            text: visible ? "Firmware: " + root.snap.power.summary + "  (" + root.snap.power.throttled + ")" : ""
            color: root.ready && root.snap.power.problem ? Kirigami.Theme.negativeTextColor : Kirigami.Theme.textColor
        }
        PlasmaComponents.Label {
            text: "argononed: " + (root.ready && root.snap.daemon_active ? "active" : "stopped")
            color: root.ready && root.snap.daemon_active ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
        }
        PlasmaComponents.Button {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: root.openApp()
        }
    }
}
