// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "../../qml/viewers"

ApplicationWindow {
    id: window
    property bool reachedEnd

    ViewerTools { id: tools }

    function waitFor(done, what) {
        for (var i = 0; i < 200 && !done(); ++i)
            tools.pump(50)
        if (!done())
            console.error("FAILED: timed out waiting for " + what)
        return done()
    }

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function find(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        var kids = item.children || []
        for (var i = 0; i < kids.length; ++i) {
            var hit = find(kids[i], name)
            if (hit)
                return hit
        }
        return null
    }

    function infoOf(page) {
        for (var i = 0; i < page.data.length; ++i)
            if (page.data[i].trackTotal !== undefined)
                return page.data[i]
        return null
    }

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("host/fixtures/tagged.mp3"), "lautta://user-documents/song.mp3"),
              "tagged fixture")
        check(tools.installFixture(Qt.resolvedUrl("host/fixtures/plain.mp3"), "lautta://user-downloads/bare-tune.mp3"),
              "plain fixture")
        var page = pageStack.push(tagged, {}, PageStackAction.Immediate)
        var cover = find(page, "coverImage"), glow = find(page, "coverGlow")
        var title = find(page, "trackTitle"), artist = find(page, "trackArtist")
        var album = find(page, "trackAlbum"), number = find(page, "trackNumber")
        check(cover && glow && title && artist && album && number, "page parts")
        waitFor(function () { return cover.status === Image.Ready && glow.status === Image.Ready }, "cover and glow")
        check(title.text === "Harbour Lights" && artist.text === "The Ferry Pilots", "title, artist: " + title.text)
        check(album.text === "Crossings · 2024", "album and year: " + album.text)
        check(number.text === "Track 3 of 11" || number.text === "lautta-viewers-media-track-of", "track: " + number.text)
        var box = find(page, "cover"), info = find(page, "trackInfo")
        check(Math.abs(box.x + box.width / 2 - page.width / 2) < 1, "cover centred across")
        check(info.y >= box.y + box.height, "track info below the cover")

        pageStack.replace(plain, {}, PageStackAction.Immediate)
        page = pageStack.currentPage
        title = find(page, "trackTitle")
        cover = find(page, "coverImage")
        var info2 = infoOf(page)
        waitFor(function () { return info2 && !info2.loading }, "tags read")
        check(title.text === "bare-tune", "file name as title: " + title.text)
        check(cover.source.toString() === "" && cover.status !== Image.Ready, "no cover anywhere, note shows")
        check(!find(page, "trackArtist").visible && !find(page, "trackNumber").visible, "missing tags take no room")
        reachedEnd = true
    }

    Component {
        id: tagged
        MediaPlayer { uri: "lautta://user-documents/song.mp3"; video: false }
    }
    Component {
        id: plain
        MediaPlayer { uri: "lautta://user-downloads/bare-tune.mp3"; video: false }
    }
}
