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
            text: root.ready && root.snap.gpu.render !== null ? "GPU " + root.pct(root.snap.gpu.render) : "GPU —"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "GPU" }
        Meter {
            Layout.fillWidth: true
            label: "render"
            visible: root.ready && root.snap.gpu.render !== null
            fraction: visible ? root.snap.gpu.render / 100 : 0
            value: visible ? root.pct(root.snap.gpu.render) : ""
        }
        Meter {
            Layout.fillWidth: true
            label: "geom"
            visible: root.ready && root.snap.gpu.bin !== null
            fraction: visible ? root.snap.gpu.bin / 100 : 0
            value: visible ? root.pct(root.snap.gpu.bin) : ""
        }
        Meter {
            Layout.fillWidth: true
            label: "CMA"
            visible: root.ready && root.snap.gpu.cma_total_mb !== null
            fraction: visible ? root.snap.gpu.cma_used_mb / root.snap.gpu.cma_total_mb : 0
            value: visible ? root.snap.gpu.cma_used_mb + "/" + root.snap.gpu.cma_total_mb + "M" : ""
        }
        PlasmaComponents.Label {
            text: root.ready && root.snap.gpu.v3d_mhz !== null ? "v3d " + root.snap.gpu.v3d_mhz + " MHz" : ""
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
