// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of CacheInfo (SEC-5 Clear cache, location list).
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property real freed: -1
    property string failure: ""

    SearchTestSupport { id: support }
    CacheInfo {
        id: cache
        onCacheCleared: root.freed = freedBytes
        onFailed: root.failure = kind
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

    }
}
