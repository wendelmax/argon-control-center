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

    toolTipMainText: "Argon GPU"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    property var hRender: []
    function record() {
        hRender = data.push(hRender, s.gpu.render !== null ? s.gpu.render : 0)
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready && root.s.gpu.render !== null ? "GPU " + data.pct(root.s.gpu.render) : "GPU —"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; text: "GPU" }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Meter {
                Layout.fillWidth: true
                visible: root.ready && root.s.gpu.render !== null
                label: "render"
                fraction: root.ready && root.s.gpu.render !== null ? root.s.gpu.render / 100 : 0
                value: root.ready ? data.pct(root.s.gpu.render) : "—"
            }
            Meter {
                Layout.fillWidth: true
                visible: root.ready && root.s.gpu.bin !== null
                label: "geom"
                fraction: root.ready && root.s.gpu.bin !== null ? root.s.gpu.bin / 100 : 0
                value: root.ready ? data.pct(root.s.gpu.bin) : "—"
            }
            Meter {
                Layout.fillWidth: true
                visible: root.ready && root.s.gpu.cma_total_mb !== null
                label: "CMA"
                fraction: root.ready && root.s.gpu.cma_total_mb ? root.s.gpu.cma_used_mb / root.s.gpu.cma_total_mb : 0
                value: root.ready && root.s.gpu.cma_total_mb ? root.s.gpu.cma_used_mb + "/" + root.s.gpu.cma_total_mb + "M" : ""
            }
        }
        Flow {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            Gauge {
                visible: root.ready && root.s.gpu.render !== null
                label: "render"
                fraction: root.ready && root.s.gpu.render !== null ? root.s.gpu.render / 100 : 0
                value: root.ready ? Math.round(root.s.gpu.render || 0) : "—"
                unit: "%"
            }
            Gauge {
                visible: root.ready && root.s.gpu.cma_total_mb !== null
                label: "CMA"
                fraction: root.ready && root.s.gpu.cma_total_mb ? root.s.gpu.cma_used_mb / root.s.gpu.cma_total_mb : 0
                value: root.ready && root.s.gpu.cma_total_mb ? Math.round(100 * root.s.gpu.cma_used_mb / root.s.gpu.cma_total_mb) : "—"
                unit: "%"
            }
        }
        Spark {
            visible: root.style === "chart"
            series: [{ values: root.hRender, color: "#27ae60", max: 100 }]
        }
        PlasmaComponents.Label {
            text: root.ready && root.s.gpu.v3d_mhz !== null ? "v3d " + root.s.gpu.v3d_mhz + " MHz" : ""
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
