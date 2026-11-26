// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root
    property bool reachedEnd

    SearchTestSupport { id: support }
    SearchModel {
        id: model
        rootUri: "lautta://user-documents/srch_t1"
    }
    Repeater {
        id: rows
        model: model
        delegate: Item {
            property string rowSection: section
            property string rowName: name
            property string rowUri: uri
            property bool rowIsDir: isDir
            property int rowStart: matchStart
            property int rowLength: matchLength
            property string rowCategory: category
            property string rowGroup: group
            property string rowPlace: place
            property string rowMime: mimeType
            property string rowTypeIcon: typeIcon
            property int rowGroupIndex: groupIndex
        }
    }

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    function waitIdle() {
        for (var i = 0; i < 3000 && model.running; ++i)
            support.spin(10)
        check(!model.running, "search finished")
    }

    function names() {
        var out = []
        for (var i = 0; i < rows.count; ++i)
            out.push(rows.itemAt(i).rowName)
        out.sort()
        return out.join(",")
    }

    function run(query, mode, types, size, date, hidden) {
        model.query = query
        model.types = JSON.stringify(types)
        model.sizePreset = size
        model.datePreset = date
        model.includeHidden = hidden
        var started = model.start()
        waitIdle()
        return started
    }

    Component.onCompleted: {
        support.prepare()
        support.write("Documents/srch_t1/Uni/thesis_draft.odt", "d", 0)
        support.write("Documents/srch_t1/Uni/Thesis/final.pdf", "ffff", 0)
        support.write("Documents/srch_t1/Uni/Thesis/Thesis_notes.md", "n", 0)
        support.write("Documents/srch_t1/Uni/.thesis_hidden", "h", 0)
        support.write("Documents/srch_t1/big_thesis.zip", new Array(2001).join("x"), 0)
        support.write("Documents/srch_t1/old_thesis.txt", "o", 1000000000)
        support.write("Documents/srch_t1/other.txt", "o", 0)

        check(model.rootName === "srch_t1", "root name: " + model.rootName)
        check(!model.canSearch(), "an empty request cannot search")
        check(!model.start(), "start refuses an empty request")
        check(!model.running, "refused start does not run")

        check(run("thesis", "substring", [], "any", "any", false), "substring search starts")
        check(names() === "Thesis,Thesis_notes.md,big_thesis.zip,old_thesis.txt,thesis_draft.odt",
              "substring hits: " + names())
        check(model.hitCount === 5, "hit count " + model.hitCount)
        check(model.folderCount >= 3, "folders searched " + model.folderCount)
        check(model.depthLimit === -1, "local search is unlimited")

        var sections = []
        for (var i = 0; i < rows.count; ++i) {
            var s = rows.itemAt(i).rowSection
            if (sections.length === 0 || sections[sections.length - 1] !== s)
                sections.push(s)
        }
        check(sections.length === 3, "one run per folder: " + sections.join("|"))
        check(sections.indexOf("srch_t1") >= 0, "root section")
        check(sections.indexOf("srch_t1 › Uni") >= 0, "nested section: " + sections.join("|"))
        check(sections.indexOf("srch_t1 › Uni › Thesis") >= 0, "deep section")

        var found = false
        for (var j = 0; j < rows.count; ++j) {
            var r = rows.itemAt(j)
            if (r.rowName === "thesis_draft.odt") {
                found = true
                check(r.rowStart === 0 && r.rowLength === 6, "highlight " + r.rowStart + "," + r.rowLength)
                check(r.rowUri === "lautta://user-documents/srch_t1/Uni/thesis_draft.odt", "hit uri " + r.rowUri)
                check(!r.rowIsDir, "file is no folder")
            }
            if (r.rowName === "Thesis")
                check(r.rowIsDir && r.rowCategory === "folder", "folder hit")
        }
        check(found, "draft found")

        check(JSON.parse(model.recentSearchesJson).length === 0,
              "a search alone is not remembered: " + model.recentSearchesJson)
        model.rememberSearch()
        var recent = JSON.parse(model.recentSearchesJson)
        check(recent.length === 1 && recent[0] === "thesis", "recent searches: " + model.recentSearchesJson)

        run("*thesis*.md", "glob", [], "any", "any", false)
        check(names() === "Thesis_notes.md", "glob hits: " + names())
        model.rememberSearch()
        check(JSON.parse(model.recentSearchesJson)[0] === "*thesis*.md", "newest recent first")

        run("thesis", "substring", ["document", "archive"], "any", "any", false)
        check(names() === "big_thesis.zip,thesis_draft.odt", "type filter: " + names())
        run("thesis", "substring", ["folder"], "any", "any", false)
        check(names() === "Thesis", "folder type: " + names())

        run("thesis", "substring", [], "gt1m", "any", false)
        check(rows.count === 0, "nothing over 1 MB")
        run("", "substring", [], "lt1m", "any", false)
        check(rows.count > 0 && model.canSearch(), "a filter alone can search")
        run("thesis", "substring", [], "any", "year", false)
        check(names().indexOf("old_thesis.txt") < 0 && names().indexOf("Thesis_notes.md") >= 0,
              "date filter keeps recent files only: " + names())

        run("thesis", "substring", [], "any", "any", true)
        check(names().indexOf(".thesis_hidden") >= 0, "hidden files with the toggle")
        run("thesis", "substring", [], "any", "any", false)
        check(names().indexOf(".thesis_hidden") < 0, "hidden files stay out by default")

        model.query = "thesis"
        model.types = "[]"
        check(model.start(), "start for cancel")
        model.cancel()
        check(!model.running, "cancel stops at once")
        var before = model.hitCount
        support.spin(300)
        check(model.hitCount === before, "no hits after cancel: " + before + " -> " + model.hitCount)
        check(rows.count === before, "no rows after cancel")

        model.query = "thes"
        model.rememberSearch()
        model.query = "thesis_dr"
        model.rememberSearch()
        check(JSON.stringify(JSON.parse(model.recentSearchesJson)) === JSON.stringify(["thesis_dr", "*thesis*.md", "thesis"]),
              "a longer search replaces the one it grew from: " + model.recentSearchesJson)

        model.clear()
        check(rows.count === 0 && model.hitCount === 0, "clear empties the list")
        model.clearRecent()
        check(JSON.parse(model.recentSearchesJson).length === 0, "recent searches cleared")

        check(model.scope === "", "a new model searches its folder")
        model.scope = "device"
        check(JSON.parse(model.recentSearchesJson).length === 0, "the device keeps its own recent searches")
        run("thesis_draft", "substring", [], "any", "any", false)
        var groups = JSON.parse(model.groupsJson)
        check(groups.length === 1 && groups[0].key === "@device" && groups[0].kind === "device",
              "device scope is one group: " + model.groupsJson)
        check(groups[0].rows === 1, "the group counts its rows: " + model.groupsJson)
        check(groups[0].state === "done" && groups[0].hits === 1 && groups[0].folders >= 4,
              "the group reports its outcome: " + model.groupsJson)
        check(rows.count === 1, "one hit on the whole device: " + names())
        var d = rows.itemAt(0)
        check(d.rowSection === "@device" && d.rowGroup === "@device" && d.rowGroupIndex === 0,
              "device rows are sectioned by group: " + d.rowSection)
        check(d.rowPlace === "Documents › srch_t1 › Uni", "place names the device folder: " + d.rowPlace)
        check(d.rowMime.length > 0 && d.rowTypeIcon.length > 0, "rows carry their look: " + d.rowMime + " " + d.rowTypeIcon)
        check(JSON.parse(model.recentSearchesJson).length === 0, "nothing remembered before asked")
        model.rememberSearch()
        check(JSON.stringify(JSON.parse(model.recentSearchesJson)) === JSON.stringify(["thesis_draft"]),
              "device recents: " + model.recentSearchesJson)
        check(!model.hasOtherPlaces(), "no servers or cards here")
        model.scope = "folder"
        check(JSON.parse(model.recentSearchesJson).indexOf("thesis_draft") < 0, "folder recents stay apart")

        run("thesis", "substring", [], "any", "any", false)
        var full = rows.count
        check(full === 5, "a finished search to narrow: " + names())
        support.write("Documents/srch_t1/thesis_late.odt", "l", 0)
        model.types = JSON.stringify(["document"])
        check(model.start(), "a narrower request starts")
        check(!model.running, "adding a type filter needs no walk")
        check(names() === "thesis_draft.odt", "the type filter narrows the finished hits only: " + names())
        model.types = "[]"
        model.query = "thesis_d"
        check(model.start() && !model.running, "a longer text needs no walk either")
        check(names() === "thesis_draft.odt", "longer text narrows: " + names())
        model.query = "thesis"
        check(model.start() && !model.running, "back to the finished request needs no walk")
        check(rows.count === full, "back to the full finished result: " + names())
        check(JSON.parse(model.groupsJson)[0].state === "done", "narrowed groups keep their state")
        model.query = "late"
        check(model.start() && model.running, "a different text walks again")
        waitIdle()
        check(names() === "thesis_late.odt", "a new walk sees new files: " + names())
        model.query = "thesis"
        check(model.start(), "start to stop")
        model.cancel()
        model.query = "thesis_l"
        check(model.start() && model.running, "a stopped search is never narrowed")
        waitIdle()
        check(names() === "thesis_late.odt", "the new walk finds it: " + names())

        model.rootUri = "lautta://user-documents/nonexistent_dir"
        run("x", "substring", [], "any", "any", false)
        check(model.errorKind.length > 0, "a missing root reports an error: " + model.errorKind)
        reachedEnd = true
    }
}
