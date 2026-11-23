// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

QtObject {
    property bool reachedEnd
    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    property var favourites: FavouritesModel { }
    property var locations: LocationsModel { }
    property var recents: RecentsModel { }

    Component.onCompleted: {
        check(Bridge.status === "absent", "status absent: " + Bridge.status)
        check(!Bridge.ready, "bridge not ready")
        check(Bridge.version === "", "no bridge version")
        check(!Bridge.storageSettings, "no Storage settings without the bridge")
        check(Bridge.connectAdHoc === undefined && Bridge.answer === undefined,
              "Lautta takes no server address, password or answer")
        check(typeof Bridge.mountDrive === "function" && typeof Bridge.unmountDrive === "function",
              "drives are mounted through the Bridge")
        Bridge.poke()
        Bridge.setForeground(true)
        Bridge.setDiscover(false)
        Bridge.addServer("smb")
        check(Bridge.status === "absent", "nothing to wake or hand off without the bridge: " + Bridge.status)

        check(favourites.count === 0 && !favourites.loaded, "favourites start empty")
        check(favourites.get(7) === "{}", "no such favourite: " + favourites.get(7))
        check(favourites.idOf("lautta://user-documents/") === -1, "not a favourite")
        check(favourites.placeOf("lautta://user-documents/") === "Documents", "place of a root: " + favourites.placeOf("lautta://user-documents/"))
        check(favourites.placeOf("lautta://user-documents/Uni/Thesis") === "Documents › Uni › Thesis", "place of a folder: " + favourites.placeOf("lautta://user-documents/Uni/Thesis"))
        check(favourites.placeOf("nonsense") === "", "place of nonsense")
        favourites.moveTo(7, 1)

        check(locations.count === 0, "locations start empty")
        check(locations.sectionCount("servers") === 0, "no servers section")
        check(locations.nearbyCount === 0, "no nearby servers")
        locations.refresh()

        check(recents.count === 0 && !recents.enabled && !recents.loaded, "recents start empty")
        check(recents.kindFilter === undefined && recents.text === undefined, "recents have no filter")

        var tile = JSON.parse(App.tileOf("lautta://user-documents/notes.md"))
        check(tile.name === "notes.md" && tile.category === "markdown" && tile.uri === "lautta://user-documents/notes.md",
              "a file's tile tells name and type: " + JSON.stringify(tile))
        check(App.tileOf("no uri") === "", "no uri, no tile")
        reachedEnd = true
    }
}
