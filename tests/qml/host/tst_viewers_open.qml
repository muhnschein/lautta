// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import "../../../qml/components/Open.js" as Open

QtObject {
    id: root

    property bool reachedEnd
    property var pushed: []

    function same(got, want, what) {
        if (JSON.stringify(got) !== JSON.stringify(want))
            console.error("FAILED: " + what + ": " + JSON.stringify(got) + ", not " + JSON.stringify(want))
    }

    property QtObject stack: QtObject {
        function push(url, props) {
            root.pushed.push({ "page": String(url).replace(/^.*\/qml\//, ""), "props": props })
        }
    }

    property QtObject app: QtObject {
        function viewerFor(uri, mimeType) {
            return { "mp4": "video", "jpg": "image", "mp3": "audio" }[uri.replace(/^.*\./, "")]
        }
        function parentUri(uri) {
            return uri.replace(/[^\/]*$/, "")
        }
    }

    function open(uri, folderUri, tileSize) {
        pushed = []
        var viewer = Open.open(stack, app, null, uri, false, "", folderUri, tileSize)
        return { "viewer": viewer, "pushed": pushed }
    }

    Component.onCompleted: {
        same(open("lautta://user-videos/a.mp4", "", 120), { "viewer": "video", "pushed": [{ "page": "viewers/ImageViewer.qml",
             "props": { "uri": "lautta://user-videos/a.mp4", "folderUri": "lautta://user-videos/", "tileSize": 120, "video": true } }] },
             "a video opens in the photos' viewer, on its folder, marked as a video")
        same(open("lautta://user-pictures/b.jpg", "lautta://search/", 0), { "viewer": "image", "pushed": [{ "page": "viewers/ImageViewer.qml",
             "props": { "uri": "lautta://user-pictures/b.jpg", "folderUri": "lautta://search/", "tileSize": 0, "video": false } }] },
             "a photo opens there as a photo, in the folder given")
        same(open("lautta://user-music/c.mp3", "", 0), { "viewer": "audio", "pushed": [{ "page": "viewers/MediaPlayer.qml",
             "props": { "uri": "lautta://user-music/c.mp3" } }] },
             "audio keeps its own player")
        reachedEnd = true
    }
}
