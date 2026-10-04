// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the browse facades without a bridge (standalone, SPEC §3.4):
// the Bridge singleton is quiet, models start empty and their synchronous
// methods and defaults behave as doc/QML-API.md says. Loading is checked
// with the real event loop in crates/lautta-qt/tests/browse_models.rs.
import QtQuick 2.6
import Lautta 1.0

QtObject {
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    property var favourites: FavouritesModel { }
    property var locations: LocationsModel { }
    property var tags: TagsModel { }
    property var items: TaggedItemsModel { }
    property var recents: RecentsModel { }
    property var prefs: LocationPrefsModel { }
    property var folder: FolderInfo { }

    Component.onCompleted: {
        Bridge.failed.connect(function(kind) { console.error("FAILED: unexpected failure " + kind) })
        Bridge.question.connect(function() { console.error("FAILED: unexpected question") })
        // No bridge socket in the temp HOME: standalone, nothing to see (UI-8).
        check(Bridge.status === "absent", "status absent: " + Bridge.status)
        check(!Bridge.ready, "bridge not ready")
        check(Bridge.version === "", "no bridge version")
        check(Bridge.pending() === "[]", "no questions waiting")
        Bridge.poke()
        Bridge.setForeground(true)
        Bridge.setDiscover(false)
        Bridge.answer("q404", JSON.stringify({ "accept": true }))

        check(favourites.count === 0 && !favourites.loaded, "favourites start empty")
        check(favourites.get(7) === "{}", "no such favourite: " + favourites.get(7))
        check(favourites.idOf("lautta://user-documents/") === -1, "not a favourite")
        check(favourites.placeOf("lautta://user-documents/") === "Documents", "place of a root: " + favourites.placeOf("lautta://user-documents/"))
        check(favourites.placeOf("lautta://user-documents/Uni/Thesis") === "Documents › Uni › Thesis", "place of a folder: " + favourites.placeOf("lautta://user-documents/Uni/Thesis"))
        check(favourites.placeOf("nonsense") === "", "place of nonsense")
        favourites.moveBy(7, 1)

        check(locations.count === 0, "locations start empty")
        check(locations.sectionCount("servers") === 0, "no servers section")
        check(locations.recentCount === 0 && locations.nearbyCount === 0, "no recent or nearby servers")
        locations.refresh()

        check(tags.count === 0, "no tags")
        check(items.tagId === 0 && items.count === 0 && items.missingCount === 0, "tag page starts empty")
        items.tagId = 3
        check(items.tagId === 3, "tag id is set")

        check(recents.count === 0 && !recents.enabled && !recents.loaded, "recents start empty")
        recents.kindFilter = "edited"
        recents.text = "notes"
        check(recents.kindFilter === "edited" && recents.text === "notes", "recents filter is kept")

        check(prefs.locationName === "" && prefs.bulkLanes === 0, "prefs start empty")
        prefs.locationId = "user-documents"
        check(prefs.locationId === "user-documents", "location id is set")

        folder.uri = "lautta://user-documents/"
        check(folder.uri === "lautta://user-documents/", "folder uri is set")
        check(folder.count === -1, "folder count is unknown until looked up")
    }
}
