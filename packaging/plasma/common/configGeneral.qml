import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import org.kde.kcmutils as KCM

// Style selector shared by the configurable widgets.
KCM.SimpleKCM {
    property string cfg_style
    property string cfg_styleDefault

    Kirigami.FormLayout {
        QQC2.ComboBox {
            id: style
            Kirigami.FormData.label: "Style:"
            textRole: "text"
            valueRole: "value"
            model: [
                { text: "Bars", value: "bars" },
                { text: "Gauges", value: "gauges" },
                { text: "Chart (last 2 minutes)", value: "chart" }
            ]
            Component.onCompleted: currentIndex = Math.max(0, indexOfValue(cfg_style))
            onActivated: cfg_style = currentValue
        }
    }
}
