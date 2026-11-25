// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/search/SearchGroups.js" as SearchGroups

QtObject {
    property bool reachedEnd
    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    function group(state, extra) {
        var g = { "key": "nv-nas", "name": "NAS", "kind": "server", "state": state, "folders": 0, "hits": 0,
                  "errors": 0, "errorKind": "" }
        for (var k in extra)
            g[k] = extra[k]
        return g
    }

    Component.onCompleted: {
        var groups = [group("done", { "key": "@device", "kind": "device", "name": "" }), group("searching")]
        same(SearchGroups.find(groups, "nv-nas"), groups[1], "find by key")
        same(SearchGroups.find(groups, "nv-office"), null, "unknown key finds nothing")
        same(SearchGroups.name(groups[0]), "lautta-search-group-device", "the device has its own name")
        same(SearchGroups.name(groups[1]), "NAS", "a server keeps its name")
        same(SearchGroups.name(null), "", "no group, no name")

        same(SearchGroups.status(group("searching")), "lautta-search-group-searching", "searching, nothing walked yet")
        same(SearchGroups.status(group("searching", { "folders": 25 })), "lautta-search-group-searching-folders",
             "searching with a folder count")
        same(SearchGroups.status(group("stopped")), "lautta-search-group-stopped", "stopped")
        same(SearchGroups.status(group("done", { "hits": 3 })), "lautta-search-group-found", "found some")
        same(SearchGroups.status(group("done")), "lautta-search-group-none", "found nothing")
        same(["TimedOut", "NetworkUnreachable", "ConnectionLost", "BridgeUnavailable", "TooManyConnections",
              "RateLimited", "LocationGone"].map(function (k) {
                  return SearchGroups.status(group("failed", { "errorKind": k }))
              }),
             ["lautta-search-group-unreachable", "lautta-search-group-unreachable", "lautta-search-group-unreachable",
              "lautta-search-group-unreachable", "lautta-search-group-unreachable", "lautta-search-group-unreachable",
              "lautta-search-group-unreachable"], "connection failures read as not reachable")
        same(SearchGroups.status(group("failed", { "errorKind": "PermissionDenied" })), "lautta-search-group-denied",
             "access refused")
        same(["AuthFailed", "ServerIdentityUnknown", "ServerIdentityChanged", "PasswordRequired", "SecurityPolicy"]
             .map(function (k) { return SearchGroups.status(group("failed", { "errorKind": k })) }),
             ["lautta-search-group-attention", "lautta-search-group-attention", "lautta-search-group-attention",
              "lautta-search-group-attention", "lautta-search-group-attention"], "sign-in trouble needs attention")
        same(SearchGroups.status(group("failed", { "errorKind": "Io" })), "lautta-search-failed", "anything else failed")
        same(SearchGroups.status(null), "", "no group, no status")
        same([SearchGroups.failed(group("failed")), SearchGroups.failed(group("done")), SearchGroups.failed(null)],
             [true, false, false], "only a failed group is failed")

        same(["folder", "device", "everywhere"].map(function (s) { return SearchGroups.scopeIcon(s) }),
             ["image://theme/icon-m-folder", "image://theme/icon-m-device", "image://theme/icon-m-website"],
             "an icon per scope")
        same(["folder", "device", "everywhere"].map(function (s) { return SearchGroups.scopeName(s, "Uni") }),
             ["Uni", "lautta-search-group-device", "lautta-search-scope-everywhere"], "a name per scope")

        var list = ["image"]
        same(SearchGroups.toggled(list, "video"), ["image", "video"], "toggling adds")
        same(SearchGroups.toggled(list, "image"), [], "toggling again removes")
        same(list, ["image"], "toggling leaves the list it got alone")
        reachedEnd = true
    }
}
