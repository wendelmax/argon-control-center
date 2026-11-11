import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponents
import org.kde.kirigami as Kirigami

PlasmoidItem {
    id: root
    readonly property var s: data.snap
    readonly property bool ready: data.ready
    readonly property var gpio: ready ? s.gpio : null
    readonly property string style: Plasmoid.configuration.style || "map"
    readonly property bool allowControl: Plasmoid.configuration.allowControl === true
    readonly property var chosen: {
        var out = []
        var parts = (Plasmoid.configuration.pins || "").split(",")
        for (var i = 0; i < parts.length; i++) {
            var n = parseInt(parts[i].trim().replace(/^gpio/i, ""))
            if (!isNaN(n) && n >= 0 && n <= 27 && out.indexOf(n) < 0) out.push(n)
        }
        return out
    }

    // Physical header, pins 1..40: [kind, bcm]. Same map as the application.
    readonly property var header: [
        ["3v3", -1], ["5v", -1], ["gpio", 2], ["5v", -1], ["gpio", 3], ["gnd", -1],
        ["gpio", 4], ["gpio", 14], ["gnd", -1], ["gpio", 15], ["gpio", 17], ["gpio", 18],
        ["gpio", 27], ["gnd", -1], ["gpio", 22], ["gpio", 23], ["3v3", -1], ["gpio", 24],
        ["gpio", 10], ["gnd", -1], ["gpio", 9], ["gpio", 25], ["gpio", 11], ["gpio", 8],
        ["gnd", -1], ["gpio", 7], ["id", 0], ["id", 1], ["gpio", 5], ["gnd", -1],
        ["gpio", 6], ["gpio", 12], ["gpio", 13], ["gnd", -1], ["gpio", 19], ["gpio", 16],
        ["gpio", 26], ["gpio", 20], ["gnd", -1], ["gpio", 21]
    ]

    ArgonData { id: data }

    toolTipMainText: "Argon GPIO"
    toolTipSubText: !ready ? "argon-control-center --json"
                  : gpio === null ? "No access to the GPIO pins"
                  : gpio.source === "service" ? "Read-only (GPIO state service)" : s.board
    preferredRepresentation: Plasmoid.formFactor === PlasmaCore.Types.Planar ? fullRepresentation : compactRepresentation

    function pinState(bcm) {
        if (gpio === null) return null
        for (var i = 0; i < gpio.pins.length; i++) if (gpio.pins[i].bcm === bcm) return gpio.pins[i]
        return null
    }
    function roleColor(kind, p) {
        if (kind === "3v3") return "#e8a33d"
        if (kind === "5v") return "#d9534f"
        if (kind === "gnd") return "#8a9099"
        if (kind === "id") return "#2aa198"
        var f = p ? p.function : ""
        if (f.indexOf("I2C") >= 0) return "#4a90d9"
        if (f.indexOf("UART") >= 0) return "#3fae6a"
        if (f.indexOf("SPI") >= 0) return "#9b6fd1"
        if (f.indexOf("PCM") >= 0) return "#d45d9a"
        if (f.indexOf("PWM") >= 0) return "#c9a227"
        if (f.indexOf("ARGON") >= 0) return "#e2683c"
        return "#7d8fa3"
    }
    function canToggle(p) {
        return allowControl && gpio !== null && gpio.writable && p !== null && p.mode === "OUT"
    }
    function toggle(p) {
        if (!canToggle(p)) return
        data.run("--gpio " + p.bcm + "=" + (p.high ? 0 : 1))
    }
    function describe(p) {
        var name = "GPIO" + p.bcm + (p.function ? " · " + p.function : "")
        return name + "\npin " + p.pin + " · " + p.mode + " · pull " + p.pull + " · " + (p.high ? "HIGH" : "LOW")
             + (canToggle(p) ? "\nClick to toggle" : "")
    }

    compactRepresentation: MouseArea {
        Layout.minimumWidth: compactRow.implicitWidth + Kirigami.Units.smallSpacing * 2
        onClicked: root.expanded = !root.expanded
        RowLayout {
            id: compactRow
            anchors.centerIn: parent
            spacing: Kirigami.Units.smallSpacing
            PlasmaComponents.Label {
                visible: root.gpio === null
                text: "GPIO —"
            }
            Repeater {
                model: root.gpio === null ? [] : root.chosen
                delegate: RowLayout {
                    required property int modelData
                    readonly property var p: root.pinState(modelData)
                    spacing: 2
                    PlasmaComponents.Label { text: modelData }
                    Rectangle {
                        implicitWidth: Kirigami.Units.iconSizes.small * 0.55
                        implicitHeight: implicitWidth
                        radius: width / 2
                        color: p && p.high ? Kirigami.Theme.positiveTextColor : "transparent"
                        border.width: 1.5
                        border.color: p && p.high ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.disabledTextColor
                    }
                }
            }
        }
    }

    fullRepresentation: Item {
        id: full
        readonly property real unit: Kirigami.Units.gridUnit
        // Height of the content for each style, so Plasma sizes the background to fit.
        readonly property real contentHeight: root.style === "map"
            ? unit * 1.8 + (unit * 1.25) * 2 + Kirigami.Units.smallSpacing + unit * 2.2
            : unit * 1.8 + Math.max(1, root.chosen.length) * unit * 2.6 + unit * 2.2
        Layout.minimumWidth: unit * (root.style === "map" ? 12 : 10)
        Layout.minimumHeight: contentHeight
        Layout.preferredWidth: unit * (root.style === "map" ? 24 : 16)
        Layout.preferredHeight: contentHeight + Kirigami.Units.largeSpacing

    ColumnLayout {
        id: content
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            Kirigami.Heading { level: 4; text: "GPIO"; Layout.fillWidth: true }
            PlasmaComponents.Label {
                visible: root.gpio !== null && root.gpio.source === "service"
                text: "read-only"
                font: Kirigami.Theme.smallFont
                opacity: 0.7
            }
        }

        PlasmaComponents.Label {
            visible: root.ready && root.gpio === null
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: "No access to the GPIO pins. Run argon-control-center --doctor for the fix."
        }

        // Header map: two rows of 20 pins, as seen from above with pin 1 on the left.
        GridLayout {
            id: map
            visible: root.style === "map" && root.gpio !== null
            Layout.alignment: Qt.AlignHCenter
            columns: 20
            rowSpacing: Kirigami.Units.smallSpacing
            columnSpacing: Kirigami.Units.smallSpacing / 2
            // From the content width (not the map's own width), so the 20 columns always fit;
            // at most 2 grid units, at least 8 px.
            readonly property real cell: Math.max(8,
                Math.min(Kirigami.Units.gridUnit * 2, Math.floor((content.width - columnSpacing * 19) / 20)))
            Repeater {
                // Row 1: even pins (2, 4 ... 40); row 2: odd pins (1, 3 ... 39).
                model: 40
                delegate: Item {
                    required property int index
                    readonly property int phys: index < 20 ? (index + 1) * 2 : (index - 20) * 2 + 1
                    readonly property var def: root.header[phys - 1]
                    readonly property var p: def[0] === "gpio" ? root.pinState(def[1]) : null
                    readonly property color role: root.roleColor(def[0], p)
                    Layout.preferredWidth: map.cell
                    Layout.preferredHeight: map.cell
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: 1
                        radius: width / 2
                        color: p === null ? parent.role : (p.high ? Kirigami.Theme.positiveTextColor : "transparent")
                        border.width: p === null || p.high ? 0 : 1.5
                        border.color: parent.role
                    }
                    PlasmaComponents.Label {
                        anchors.centerIn: parent
                        text: parent.phys
                        font.pixelSize: Math.max(7, map.cell * 0.38)
                        color: p !== null && !p.high ? Kirigami.Theme.textColor : "white"
                    }
                    MouseArea {
                        id: hit
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.canToggle(parent.p) ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: root.toggle(parent.p)
                    }
                    QQC2.ToolTip.visible: hit.containsMouse
                    QQC2.ToolTip.text: p !== null ? root.describe(p)
                        : def[0] === "id" ? "ID EEPROM · pin " + phys
                        : (def[0] === "3v3" ? "3.3V" : def[0] === "5v" ? "5V" : "GND") + " · pin " + phys
                }
            }
        }

        // List of chosen pins.
        Repeater {
            model: root.style === "list" && root.gpio !== null ? root.chosen : []
            delegate: RowLayout {
                required property int modelData
                readonly property var p: root.pinState(modelData)
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Rectangle {
                    implicitWidth: 3
                    Layout.fillHeight: true
                    radius: 1
                    color: root.roleColor("gpio", p)
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    PlasmaComponents.Label {
                        text: "GPIO" + modelData + (p && p.function ? " · " + p.function : "")
                        font.bold: true
                    }
                    PlasmaComponents.Label {
                        text: p ? "pin " + p.pin + " · " + p.mode + " · pull " + p.pull : "not reported"
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                }
                PlasmaComponents.Label {
                    text: p ? (p.high ? "HIGH" : "LOW") : "—"
                    color: p && p.high ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.textColor
                    font.bold: true
                }
                PlasmaComponents.ToolButton {
                    visible: root.canToggle(p)
                    text: "Toggle"
                    onClicked: root.toggle(p)
                }
            }
        }

        Item { Layout.fillHeight: true }

        PlasmaComponents.ToolButton {
            Layout.alignment: Qt.AlignRight
            text: "GPIO Lab"
            icon.name: "argon-control-center"
            onClicked: data.openApp()
        }
    }
    }
}
