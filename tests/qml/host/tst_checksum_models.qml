// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0
import "../../../qml/components/operations/Json.js" as Json

Item {
    id: root
    property bool reachedEnd
    readonly property string docs: "lautta://user-documents/"
    readonly property string longName: new Array(301).join("n")
    readonly property string photoSha256: "eaf602095e76dac2dfdd976674990dbf2dede5e706d35f0aee2a9368b6407158"

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function same(a, b, what) {
        check(JSON.stringify(a) === JSON.stringify(b), what + ": " + JSON.stringify(a) + " != " + JSON.stringify(b))
    }

    function until(cond, what) {
        for (var i = 0; i < 400 && !cond(); ++i)
            pump.pump(25)
        check(cond(), "timed out waiting for " + what)
    }

    function install(fixture, name) {
        check(tools.installFixture(Qt.resolvedUrl("fixtures/" + fixture), root.docs + name), "fixture " + name)
    }

    function ids(group) {
        return Json.value(App.checksumAlgorithms(group), []).map(function (a) { return a.id })
    }

    function rows() {
        var out = []
        for (var i = 0; i < rep.count; ++i)
            out.push(rep.itemAt(i).nm + ":" + rep.itemAt(i).st + ":" + rep.itemAt(i).pr + ":" + JSON.stringify(rep.itemAt(i).ek))
        return out
    }

    PickerRootsModel { id: pump }
    ViewerTools { id: tools }

    ChecksumModel { id: sum }
    VerifyModel { id: verify }
    Repeater {
        id: rep
        model: verify
        delegate: Item {
            property string nm: name
            property string st: model.state
            property var pr: model.progress
            property var ek: model.errorKind
        }
    }

    function testApp() {
        same(ids("common"), ["sha256", "md5", "sha1", "sha512"], "the most used four")
        same(ids("more"), ["sha224", "sha384", "sha3-256", "sha3-512", "blake2b", "blake3", "crc32", "xxh64", "xxh128"],
             "section More")
        check(App.isChecksumList(root.docs + "SHA256SUMS"), "SHA256SUMS is a list")
        check(App.isChecksumList(root.docs + "image.iso.sha256"), "x.sha256 is a list")
        check(!App.isChecksumList(root.docs + "photo.jpg"), "a photo is not")
    }

    function testChecksum() {
        sum.uri = root.docs + "photo.jpg"
        sum.algorithm = "sha256"
        check(sum.hex === "" && !sum.running, "nothing runs before start")
        check(sum.start(), "starts")
        check(sum.running, "running at once")
        until(function () { return !sum.running }, "SHA-256")
        same([sum.hex, sum.verdict, sum.verdictList, sum.label, sum.progress, sum.errorKind],
             [root.photoSha256, "matches", "SHA256SUMS", "SHA-256", 1, ""], "SHA-256 of the photo, list matches")

        sum.algorithm = "md5"
        check(sum.start(), "starts again")
        check(sum.hex === "" && sum.verdict === "", "a new run clears the last result")
        until(function () { return !sum.running }, "MD5")
        same([sum.hex, sum.verdict, sum.verdictList], ["153f8e9d69ab9e0dad0108627bffe34d", "differs", "photo.jpg.md5"],
             "MD5, the one-hash file differs")

        sum.algorithm = "blake3"
        check(sum.start(), "starts BLAKE3")
        until(function () { return !sum.running }, "BLAKE3")
        same([sum.hex.length, sum.verdict], [64, ""], "no list names BLAKE3: no verdict")

        sum.algorithm = "sha1"
        check(sum.start(), "starts SHA-1")
        sum.algorithm = "sha512"
        check(sum.start(), "starts SHA-512 at once, dropping SHA-1")
        until(function () { return !sum.running }, "SHA-512")
        same([sum.label, sum.hex.length, sum.errorKind], ["SHA-512", 128, ""], "only the last run shows")

        sum.uri = root.docs + "nope.bin"
        check(sum.start(), "starts on a missing file")
        until(function () { return !sum.running }, "missing file")
        same([sum.hex, sum.errorKind], ["", "NotFound"], "missing file: error, no value")
        check(sum.errorMessage.length > 0, "missing file: words")

        sum.uri = root.docs + "photo.jpg"
        sum.algorithm = "rot13"
        check(!sum.start(), "unknown algorithm refused")
    }

    function testVerify() {
        verify.uri = root.docs + "SHA256SUMS"
        check(verify.start(), "verify starts")
        until(function () { return !verify.running }, "verify")
        same(rows(), ["empty.txt:differs:1:\"\"", "missing.bin:missing:0:\"\"", root.longName + ":failed:0:\"InvalidName\"", "photo.jpg:matches:1:\"\""], "problems first, in order found")
        same([verify.count, verify.checked, verify.errorKind], [4, 4, ""], "all four checked")

        check(verify.start() && verify.start(), "verify starts twice at once")
        until(function () { return !verify.running }, "verify again")
        same([rows(), verify.checked, verify.errorKind], [["empty.txt:differs:1:\"\"", "missing.bin:missing:0:\"\"", root.longName + ":failed:0:\"InvalidName\"", "photo.jpg:matches:1:\"\""], 4, ""],
             "only the last run shows")

        verify.uri = root.docs + "photo.jpg"
        check(verify.start(), "verify starts on a photo")
        until(function () { return !verify.running }, "verify a photo")
        same([verify.count, verify.errorKind], [0, "Unsupported"], "a photo is no list")
    }

    Component.onCompleted: {
        install("photo.jpg", "photo.jpg")
        install("empty.txt", "empty.txt")
        install("SHA256SUMS-failing", "SHA256SUMS")
        install("wrong.md5", "photo.jpg.md5")
        testApp()
        testChecksum()
        testVerify()
        reachedEnd = true
    }
}
