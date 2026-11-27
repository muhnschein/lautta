// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    readonly property string tagged: "lautta://user-documents/tagged.mp3"
    readonly property string plain: "lautta://user-downloads/plain.mp3"
    readonly property string junk: "lautta://user-pictures/junk.mp3"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function read(info) {
        for (var i = 0; i < 600 && info.loading; ++i)
            tools.pump(50)
    }

    function settle(item) {
        for (var i = 0; i < 600 && item.status === Image.Loading; ++i)
            tools.pump(50)
    }

    ViewerTools { id: tools }
    AudioInfo { id: info }
    Image { id: cover; sourceSize.width: 96; sourceSize.height: 96 }
    Image { id: glow; sourceSize.width: 48; sourceSize.height: 48 }

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/tagged.mp3"), tagged), "tagged fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/plain.mp3"), plain), "plain fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/photo.jpg"), "lautta://user-downloads/Cover.JPG"),
              "folder cover fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/data.bin"), junk), "junk fixture")

        check(!info.loading && info.title === "" && info.cover === "", "nothing before a uri")
        info.uri = tagged
        check(info.loading, "reading starts at once")
        read(info)
        check(info.title === "Harbour Lights" && info.artist === "The Ferry Pilots" && info.album === "Crossings",
              "text tags: " + info.title + " / " + info.artist + " / " + info.album)
        check(info.year === 2024 && info.track === 3 && info.trackTotal === 11,
              "numbers: " + info.year + " " + info.track + "/" + info.trackTotal)
        check(info.cover === "file", "embedded cover: " + info.cover)
        cover.source = "image://lautta-cover/" + tagged + "?size=96"
        settle(cover)
        check(cover.status === Image.Ready && cover.implicitWidth > 0, "embedded cover drawn: " + cover.status)
        glow.source = "image://lautta-cover/" + tagged + "?size=48&glow=1"
        settle(glow)
        check(glow.status === Image.Ready && glow.implicitWidth === 48 && glow.implicitHeight === 48,
              "glow " + glow.implicitWidth + "x" + glow.implicitHeight)

        info.uri = plain
        check(info.loading && info.title === "" && info.year === 0, "old tags gone at once")
        read(info)
        check(info.title === "" && info.track === 0 && info.cover === "folder", "folder cover: " + info.cover)
        cover.source = "image://lautta-cover/" + plain + "?size=96"
        settle(cover)
        check(cover.status === Image.Ready, "folder cover drawn: " + cover.status)

        info.uri = junk
        read(info)
        check(info.title === "" && info.cover === "", "not audio: no tags, no cover: " + info.cover)

        info.uri = "lautta://user-documents/missing.mp3"
        read(info)
        check(info.title === "" && info.cover === "", "missing file: page shows the file name")
        info.uri = "garbage"
        check(!info.loading && info.cover === "", "bad uri clears")
        reachedEnd = true
    }
}
