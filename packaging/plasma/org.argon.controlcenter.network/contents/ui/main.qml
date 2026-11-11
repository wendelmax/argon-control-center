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

    toolTipMainText: "Argon Network"
    toolTipSubText: ready ? s.board : "argon-control-center --json"
    // Gauges float on the desktop without the widget frame.
    Plasmoid.backgroundHints: root.style === "gauges" ? PlasmaCore.Types.NoBackground : PlasmaCore.Types.DefaultBackground
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation
    property var hRx: []
    property var hTx: []
    // Bars and gauges are scaled to the peak seen in the history (at least 100 KB/s).
    readonly property real peak: Math.max(100, Math.max.apply(null, hRx.concat(hTx, [0])))
    function record() {
        hRx = data.push(hRx, s.net.rx_kbps)
        hTx = data.push(hTx, s.net.tx_kbps)
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactLabel.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        PlasmaComponents.Label {
            id: compactLabel
            anchors.centerIn: parent
            text: root.ready ? "⬇" + root.s.net.rx_kbps.toFixed(0) + " ⬆" + root.s.net.tx_kbps.toFixed(0) + " KB/s" : "—"
        }
    }

    fullRepresentation: ColumnLayout {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 14
        Layout.preferredWidth: Kirigami.Units.gridUnit * 18
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading { level: 4; visible: root.style !== "gauges"; text: "Network" }
        GaugeGrid {
            visible: root.style === "gauges"
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: Kirigami.Units.gridUnit * 4
            Layout.preferredHeight: Kirigami.Units.gridUnit * 8
            items: (root.ready ? [
                { label: "rx", fraction: root.s.net.rx_kbps / root.peak, value: root.s.net.rx_kbps.toFixed(0), unit: "KB/s" },
                { label: "tx", fraction: root.s.net.tx_kbps / root.peak, value: root.s.net.tx_kbps.toFixed(0), unit: "KB/s" }] : [])
            onActivated: data.openApp()
        }
        ColumnLayout {
            visible: root.style === "bars" || root.style === ""
            Layout.fillWidth: true
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "rx"
                fraction: root.ready ? root.s.net.rx_kbps / root.peak : 0
                value: root.ready ? root.s.net.rx_kbps.toFixed(1) + " KB/s" : "—"
            }
            Meter {
                Layout.fillWidth: true
                visible: true
                label: "tx"
                fraction: root.ready ? root.s.net.tx_kbps / root.peak : 0
                value: root.ready ? root.s.net.tx_kbps.toFixed(1) + " KB/s" : "—"
            }
        }
        Spark {
            visible: root.style === "chart"
            series: [{ values: root.hRx, color: "#3daee9", max: root.peak }, { values: root.hTx, color: "#27ae60", max: root.peak }]
        }
        PlasmaComponents.Label {
            text: "peak " + root.peak.toFixed(0) + " KB/s"
            opacity: 0.7
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
