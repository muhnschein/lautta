// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of CacheInfo (SEC-5 Clear cache, location list) and
// LocationPrefsModel (§18 per-location settings).
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property real freed: -1
    property string failure: ""
    property int saves: 0

    SearchTestSupport { id: support }
    CacheInfo {
        id: cache
        onCacheCleared: root.freed = freedBytes
        onFailed: root.failure = kind
    }
    LocationPrefsModel {
        id: prefs
        onSaved: root.saves += 1
    }

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function until(cond, ms) {
        for (var i = 0; i < ms / 10 && !cond(); ++i)
            support.spin(10)
        return cond()
    }

    Component.onCompleted: {
        support.prepare()
        var dir = ".cache/org.netvfs/lautta/"
        support.write(dir + "thumbs/a.thumb", new Array(1001).join("x"), 0)
        support.write(dir + "archives/x/file", new Array(501).join("y"), 0)
        support.write(dir + "crash/keep.txt", "report", 0)
        support.write("Documents/srch_storage_keep.txt", "mine", 0)

        cache.refresh()
        check(until(function () { return cache.thumbnailBytes >= 1000 }, 3000), "thumbnail size known: " + cache.thumbnailBytes)
        check(cache.archiveBytes >= 500, "archive size " + cache.archiveBytes)
        check(cache.cacheBytes >= 1500, "total " + cache.cacheBytes)

        cache.clearCache()
        check(until(function () { return root.freed >= 0 }, 3000), "cache cleared: " + root.failure)
        check(root.freed >= 1500, "freed bytes " + root.freed)
        check(!support.exists(dir + "thumbs/a.thumb"), "thumbnails removed")
        check(!support.exists(dir + "archives/x/file"), "archive cache removed")
        check(support.exists(dir + "crash/keep.txt"), "crash reports are kept")
        check(support.exists("Documents/srch_storage_keep.txt"), "user files are kept")
        check(until(function () { return cache.thumbnailBytes === 0 && !cache.busy }, 3000), "sizes refreshed after clearing")

        var locations = JSON.parse(cache.locationsJson())
        check(Array.isArray(locations), "locations list")
        for (var i = 0; i < locations.length; ++i)
            check(["server", "adhoc", "volume"].indexOf(locations[i].kind) >= 0, "only network and volume locations: " + locations[i].kind)

        // Per-location preferences round trip.
        prefs.locationId = "user-documents"
        check(until(function () { return prefs.ready }, 3000), "prefs loaded")
        check(prefs.defaultName === "Documents", "default name " + prefs.defaultName)
        check(prefs.displayName === "" && !prefs.noListingCache && prefs.laneSize === 0, "defaults")
        prefs.displayName = "Papers"
        prefs.noListingCache = true
        prefs.noThumbCache = true
        prefs.laneSize = 4
        prefs.startFolder = "lautta://user-documents/srch_t1"
        prefs.save()
        check(until(function () { return root.saves === 1 }, 3000), "prefs saved: " + root.failure)
        prefs.displayName = "changed in memory"
        prefs.load()
        check(until(function () { return prefs.displayName === "Papers" }, 3000), "prefs read back")
        check(prefs.noListingCache && prefs.noThumbCache && prefs.laneSize === 4, "flags and lanes kept")
        check(prefs.startFolder === "lautta://user-documents/srch_t1", "start folder kept")
        prefs.reset()
        check(until(function () { return root.saves === 2 }, 3000), "reset saved")
        check(prefs.displayName === "" && !prefs.noListingCache && prefs.laneSize === 0 && prefs.startFolder === "", "reset to defaults")
    }
}
