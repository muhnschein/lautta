// SPDX-License-Identifier: LGPL-2.1-or-later
// Starts an image's decode at finger down, before the image viewer opens: the viewer's photo
// (same source, size, autoTransform; see ViewerImage.js) takes it from Qt's pixmap cache. Hold or
// press cancelled (scroll, finger let go elsewhere): `stop()` drops it. Done or failed: dropped too,
// Qt keeps a decode ~30 s.
import QtQuick 2.6
import Sailfish.Silica 1.0
import Lautta 1.0
import "ViewerImage.js" as ViewerImage

Image {
    objectName: "viewerPrefetch"
    visible: false
    autoTransform: true
    cache: true
    sourceSize.width: Screen.width
    sourceSize.height: Screen.height

    function start(uri, mimeType) {
        source = App.viewerFor(uri, mimeType || "") === "image" ? ViewerImage.source(uri) : ""
    }

    function stop() {
        source = ""
    }

    onStatusChanged: {
        if (status !== Image.Loading)
            source = ""
    }
}
