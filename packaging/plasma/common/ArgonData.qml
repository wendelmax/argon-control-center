import QtQuick
import org.kde.plasma.plasma5support as P5Support

// Shared data source: runs `argon-control-center --json` every 2 s and keeps
// a short history (last 60 samples, about 2 minutes) for the chart style.
Item {
    id: data
    property var snap: null
    readonly property bool ready: snap !== null
    property int historyLength: 60
    // Per-user installs live in ~/.local/bin, which is often not in plasmashell's PATH.
    readonly property string cmd: "PATH=\"$PATH:$HOME/.local/bin\" argon-control-center"
    signal updated()

    function pct(v) { return (v === null || v === undefined) ? "—" : Math.round(v) + "%" }
    function push(list, value) {
        var out = list.slice(Math.max(0, list.length - historyLength + 1))
        out.push(value)
        return out
    }
    function openApp() { launcher.connectSource(cmd + " --gui >/dev/null 2>&1 &") }
    // Runs the CLI with fixed arguments built by the widget (numbers only, never user text).
    function run(args) { launcher.connectSource(cmd + " " + args + " >/dev/null 2>&1") }
    function uptime(s) {
        var d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60)
        return (d > 0 ? d + "d " : "") + h + "h " + m + "m"
    }

    P5Support.DataSource {
        engine: "executable"
        connectedSources: [data.cmd + " --json"]
        interval: 2000
        onNewData: (source, d) => {
            try {
                data.snap = JSON.parse(d["stdout"])
                data.updated()
            } catch (e) {
                data.snap = null
            }
        }
    }
    P5Support.DataSource {
        id: launcher
        engine: "executable"
        onNewData: (source) => disconnectSource(source)
    }
}
