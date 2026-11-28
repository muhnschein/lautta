// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/search/IndexText.js" as IndexText

QtObject {
    property bool reachedEnd
    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    function line(location, kind, state, names, checked, wanted) {
        return { "location": location, "name": location.toUpperCase(), "kind": kind, "local": kind !== "server",
                 "state": state, "names": names, "checkedMs": checked, "wanted": wanted }
    }

    Component.onCompleted: {
        var rows = IndexText.rows([
            line("user-documents", "device", "ready", 100, 50, true),
            line("vol-sd", "volume", "crawling", 7, -1, true),
            line("user-pictures", "device", "crawling", 20, 40, true),
            line("nv-nas", "server", "off", 0, -1, false)
        ])
        same(rows.map(function (r) { return r.kind }), ["device", "volume", "server"], "device folders make one row")
        same([rows[0].names, rows[0].state, rows[0].checkedMs], [120, "crawling", 40],
             "the device row adds names and shows its slowest folder")
        same([rows[1].name, rows[2].wanted], ["VOL-SD", false], "cards and servers keep their own rows")
        same(IndexText.rows([line("user-a", "device", "ready", 1, 9, true), line("user-b", "device", "failed", 1, 9, true)])[0].state,
             "failed", "a failed folder marks the device")
        same(IndexText.rows([line("user-a", "device", "failed", 1, 9, true), line("user-b", "device", "nospace", 1, 9, true)])[0].state,
             "failed", "a full index marks the device as failing too")
        same(IndexText.rows([line("user-a", "device", "ready", 1, 9, true), line("user-b", "device", "nospace", 1, 9, true)])[0].state,
             "nospace", "a full index marks the device")
        same(IndexText.name(rows[0]), "lautta-search-group-device", "the device row has its own name")
        same(["ready", "crawling", "failed", "off", "pending", "wifi", "nospace"].map(function (s) {
            return IndexText.status({ "state": s, "names": 3 })
        }), ["lautta-index-state-ready", "lautta-index-state-updating", "lautta-index-state-unreachable",
              "lautta-index-state-off", "lautta-index-state-waiting", "lautta-index-state-wifi",
              "lautta-index-state-no-space"],
             "a status word per state, no counts")
        reachedEnd = true
    }
}
