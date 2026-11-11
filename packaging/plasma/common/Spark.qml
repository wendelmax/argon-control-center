import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

// History chart: one filled line per series, oldest sample on the left.
// series: [{ values: [..], color: "#hex", max: 100 }]
Canvas {
    id: spark
    property var series: []
    Layout.fillWidth: true
    Layout.preferredHeight: Kirigami.Units.gridUnit * 4
    onSeriesChanged: requestPaint()
    onWidthChanged: requestPaint()
    onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        ctx.fillStyle = Qt.rgba(0.5, 0.5, 0.5, 0.08)
        ctx.fillRect(0, 0, width, height)
        for (var s = 0; s < series.length; s++) {
            var v = series[s].values, max = series[s].max > 0 ? series[s].max : 100
            if (!v || v.length < 2) continue
            var step = width / (v.length - 1)
            var y = function (x) { return height - Math.max(0, Math.min(1, x / max)) * (height - 2) - 1 }
            ctx.beginPath()
            ctx.moveTo(0, height)
            for (var i = 0; i < v.length; i++) ctx.lineTo(i * step, y(v[i]))
            ctx.lineTo(width, height)
            ctx.closePath()
            ctx.fillStyle = Qt.rgba(Qt.color(series[s].color).r, Qt.color(series[s].color).g, Qt.color(series[s].color).b, 0.15)
            ctx.fill()
            ctx.beginPath()
            for (var j = 0; j < v.length; j++) {
                if (j === 0) ctx.moveTo(0, y(v[0])); else ctx.lineTo(j * step, y(v[j]))
            }
            ctx.strokeStyle = series[s].color
            ctx.lineWidth = 1.5
            ctx.stroke()
        }
    }
}
