import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM

KCM.SimpleKCM {
    property string cfg_style
    property string cfg_styleDefault
    property string cfg_pins
    property string cfg_pinsDefault
    property bool cfg_allowControl
    property bool cfg_allowControlDefault

    Kirigami.FormLayout {
        QQC2.ComboBox {
            Kirigami.FormData.label: "Style:"
            textRole: "text"
            valueRole: "value"
            model: [
                { text: "Header map (40 pins)", value: "map" },
                { text: "List of chosen pins", value: "list" }
            ]
            Component.onCompleted: currentIndex = Math.max(0, indexOfValue(cfg_style))
            onActivated: cfg_style = currentValue
        }
        QQC2.TextField {
            Kirigami.FormData.label: "Pins (BCM):"
            placeholderText: "17,27,22"
            text: cfg_pins
            onTextEdited: cfg_pins = text
        }
        QQC2.Label {
            text: "Shown in the list style and in a panel."
            font: Kirigami.Theme.smallFont
            opacity: 0.7
        }
        QQC2.CheckBox {
            Kirigami.FormData.label: "Control:"
            text: "Clicking an output pin toggles it"
            checked: cfg_allowControl
            onToggled: cfg_allowControl = checked
        }
        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            wrapMode: Text.WordWrap
            text: "Only pins already set as outputs are changed; inputs and pins used by I2C or UART are never touched. Needs pinctrl without a password (/dev/gpiomem)."
            font: Kirigami.Theme.smallFont
            opacity: 0.7
        }
    }
}
