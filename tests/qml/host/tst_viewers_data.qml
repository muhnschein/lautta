// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of ExifModel and ImageListModel (PRV-4)
// on files in the temporary home.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    ViewerTools { id: tools }
    ExifModel { id: exif }
    ImageListModel { id: images }

    Repeater {
        id: imageRows
        model: images
        delegate: Item {
            property string rowUri: uri
            property string rowName: name
        }
    }

    Component.onCompleted: {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/data.bin"), "lautta://user-documents/data.bin"), "bin fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/photo.jpg"), "lautta://user-pictures/photo.jpg"), "jpeg fixture")

        // EXIF.
        exif.uri = "lautta://user-pictures/photo.jpg"
        check(exif.loading, "exif loading")
        tools.pump(300)
        var info = JSON.parse(exif.infoJson)
        check(exif.hasExif && info.hasExif, "has exif")
        check(info.make === "Jolla" && info.model === "Jolla C2", "camera " + exif.infoJson)
        check(info.iso === 50 && info.orientation === 6 && info.exposure === "1/640 s", "exposure fields " + exif.infoJson)
        check(info.aperture === "f/1.8" && info.dateTaken.indexOf("2026") >= 0, "aperture and date")
        check(Math.abs(info.latitude - 60.15) < 0.001 && Math.abs(info.longitude - 24.95) < 0.001, "gps")
        check(info.name === "photo.jpg" && info.size > 0, "file facts")
        exif.uri = "lautta://user-documents/data.bin"
        tools.pump(300)
        check(!exif.hasExif && JSON.parse(exif.infoJson).hasExif === false, "no exif in a binary file")
        exif.uri = "lautta://user-pictures/nope.jpg"
        tools.pump(300)
        check(exif.errorKind === "NotFound", "exif error")

        // The folder's images.
        images.folderUri = "lautta://user-pictures/"
        tools.pump(300)
        check(images.count === 1 && imageRows.count === 1, "one image in the folder " + images.count)
        check(imageRows.itemAt(0).rowUri === "lautta://user-pictures/photo.jpg" && imageRows.itemAt(0).rowName === "photo.jpg", "image row")
        check(images.indexOf("lautta://user-pictures/photo.jpg") === 0 && images.indexOf("lautta://user-pictures/other.jpg") === -1, "indexOf")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/photo.jpg"), "lautta://user-pictures/a.jpg"), "second image")
        images.reload()
        tools.pump(300)
        check(images.count === 2 && imageRows.itemAt(0).rowName === "a.jpg", "reload sees the new image, in name order")
        images.folderUri = "lautta://user-pictures/nope/"
        tools.pump(300)
        check(images.errorKind === "NotFound", "image list error " + images.errorKind)
    }
}
