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
        Layout.minimumWidth: label.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: label
            anchors.centerIn: parent
            text: root.ready ? "CPU " + root.pct(root.snap.cpu.total) : "CPU —"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "CPU" }
        Meter {
            Layout.fillWidth: true
            label: "total"
            fraction: root.ready ? root.snap.cpu.total / 100 : 0
            value: root.ready ? root.pct(root.snap.cpu.total) : "—"
        }
        Repeater {
            model: root.ready ? root.snap.cpu.cores : []
            Meter {
                Layout.fillWidth: true
                label: "cpu" + index
                fraction: modelData / 100
                value: root.pct(modelData)
            }
        }
        PlasmaComponents.Label {
            text: root.ready ? root.snap.cpu.mhz + " MHz · " + root.snap.cpu.governor : ""
            opacity: 0.7
        }
        PlasmaComponents.Button {
            Layout.alignment: Qt.AlignRight
            text: "Argon Control Center"
            icon.name: "utilities-system-monitor"
            onClicked: root.openApp()
        }
    }
}
