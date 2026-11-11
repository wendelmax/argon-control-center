import QtQuick
import QtQuick.Layouts
import org.kde.plasma.components as PlasmaComponents

// btop-style meter: label, bar filled left to right (0 -> 100 %) with a
// green -> amber -> red gradient by position, and the value.
RowLayout {
    id: meter
    property string label: ""
    property real fraction: 0
    property string value: ""
    spacing: 6

    PlasmaComponents.Label {
        text: meter.label
        font.family: "monospace"
        Layout.preferredWidth: 52
        elide: Text.ElideRight
    }
    Item {
        Layout.fillWidth: true
        Layout.preferredHeight: 10
        Layout.minimumWidth: 40
        Rectangle {
            anchors.fill: parent
            radius: 2
            color: Qt.rgba(0.5, 0.5, 0.5, 0.25)
        }
        Item {
            width: parent.width * Math.max(0, Math.min(1, meter.fraction))
            height: parent.height
            clip: true
            Rectangle {
                width: meter.width > 0 ? parent.parent.width : 0
                height: parent.height
                radius: 2
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: "#27ae60" }
                    GradientStop { position: 0.5; color: "#f39c12" }
                    GradientStop { position: 1.0; color: "#da4453" }
                }
            }
        }
    }
    PlasmaComponents.Label {
        text: meter.value
        font.family: "monospace"
        horizontalAlignment: Text.AlignRight
        Layout.preferredWidth: 78
    }
}
