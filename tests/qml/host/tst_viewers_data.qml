// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of HexModel, SqliteModel, ExifModel and ImageListModel (PRV-4)
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
    HexModel { id: hex }
    SqliteModel { id: db }
    SqliteModel { id: remoteDb }
    ExifModel { id: exif }
    ImageListModel { id: images }

    Repeater {
        id: hexRows
        model: hex
        delegate: Item {
            property string offsetValue: offsetText
            property string hexValue: model.hex
            property string asciiValue: model.ascii
            property bool ready: loaded
        }
    }

    Repeater {
        id: dbRows
        model: db
        delegate: Item {
            property var cells: JSON.parse(cellsJson)
            property int rowNumber: number
        }
    }

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
        check(tools.installFixture(Qt.resolvedUrl("fixtures/sample.sqlite"), "lautta://user-documents/sample.sqlite"), "sqlite fixture")
        check(tools.installFixture(Qt.resolvedUrl("fixtures/photo.jpg"), "lautta://user-pictures/photo.jpg"), "jpeg fixture")

        // Hex: rows are fetched when the view asks for them.
        check(hex.rowOfOffset("10") === -1, "no row before a file is open")
        hex.uri = "lautta://user-documents/data.bin"
        tools.pump(400)
        check(hex.count === 63 && hex.size === 1000, "hex count " + hex.count + " size " + hex.size)
        check(hexRows.count === 63, "delegates " + hexRows.count)
        var first = hexRows.itemAt(0)
        check(first.ready && first.offsetValue === "00000000", "first row offset")
        check(first.hexValue.indexOf("00 01 02 03 04 05 06 07") === 0, "first row hex: " + first.hexValue)
        check(first.asciiValue.length === 16 && first.asciiValue.charAt(0) === ".", "ascii")
        var last = hexRows.itemAt(62)
        check(last.ready && last.offsetValue === "000003e0", "last row offset " + last.offsetValue)
        check(hex.rowOfOffset("3e7") === 62 && hex.rowOfOffset("0x3E7") === 62, "row of the last byte")
        check(hex.rowOfOffset("10") === 1, "row of offset 0x10")
        check(hex.rowOfOffset("3e8") === -1 && hex.rowOfOffset("zz") === -1 && hex.rowOfOffset("") === -1, "bad offsets")
        hex.uri = "lautta://user-documents/nope.bin"
        tools.pump(300)
        check(hex.errorKind === "NotFound" && hex.count === 0, "hex error " + hex.errorKind)

        // SQLite: tables, paged rows.
        db.uri = "lautta://user-documents/sample.sqlite"
        check(db.loading, "sqlite loading")
        tools.pump(500)
        var tables = JSON.parse(db.tablesJson)
        check(tables.length === 2 && tables[0].name === "measurements" && tables[1].name === "notes", "tables " + db.tablesJson)
        check(tables[0].rows === 120 && tables[0].rowsExact && tables[0].columns[0].primaryKey, "table info")
        check(db.table === "measurements", "first table selected")
        check(JSON.stringify(JSON.parse(db.columnsJson)) === '["id","run","t","value","unit"]', "columns " + db.columnsJson)
        check(db.count === 50 && db.hasMore && db.totalRows === 120, "first page " + db.count)
        check(dbRows.count === 50 && dbRows.itemAt(0).cells[0] === "1200" && dbRows.itemAt(0).rowNumber === 1, "first row")
        db.loadMore()
        tools.pump(300)
        check(db.count === 100 && db.hasMore, "second page " + db.count)
        db.loadMore()
        tools.pump(300)
        check(db.count === 120 && !db.hasMore, "last page " + db.count)
        db.loadMore()
        check(db.count === 120, "nothing more to load")
        db.selectTable("notes")
        tools.pump(300)
        check(db.table === "notes" && db.count === 2 && dbRows.count === 2, "other table " + db.count)
        check(dbRows.itemAt(1).cells[1] === "NULL", "NULL cell")
        db.selectTable("no_such_table")
        check(db.table === "notes", "unknown table ignored")
        remoteDb.uri = "lautta://nv-nothing/x.sqlite"
        check(remoteDb.errorKind !== "" && !remoteDb.loading, "remote databases are refused: " + remoteDb.errorKind)

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

        // Tools.
        check(tools.uriForFileUrl("file://" + App.localUrl("lautta://user-pictures/a.jpg").replace("file://", "")) === "lautta://user-pictures/a.jpg", "uriForFileUrl")
        check(tools.uriForFileUrl("file:///nowhere/x") === "" && tools.uriForFileUrl("http://x") === "", "uriForFileUrl outside the locations")
    }
}
