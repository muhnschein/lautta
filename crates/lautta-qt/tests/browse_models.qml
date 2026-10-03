// The scenario of crates/lautta-qt/tests/browse_models.rs: steps with an
// action and a condition; a step fails when its condition does not hold
// within five seconds. Qt 5.15 on the host, so no Connections here.
Item {
    id: root

    property int stage: 0
    property int waited: 0
    property bool acted: false
    property var steps: []
    property string questionKind: ""
    property string questionHost: ""
    property string connected: ""
    property string failedKind: ""
    property string failedSubject: ""
    property int removedSignals: 0

    LocationsModel { id: locs }
    FavouritesModel { id: favs }
    TagsModel { id: tagsModel }
    TaggedItemsModel { id: tagged; onRemoved: root.removedSignals++ }
    RecentsModel { id: recents }
    LocationPrefsModel { id: prefs }

    Repeater {
        id: rows
        model: locs
        delegate: Item {
            property string rkind: kind
            property string rname: name
            property string rstatus: status
            property string rattention: attention
            property string rcolour: colour
            property string rplace: place
            property string rprovider: provider
            property string ruri: uri
            property string ritem: itemId
            property int rcount: count
        }
    }

    Repeater {
        id: recentRows
        model: recents
        delegate: Item {
            property int rid: itemId
            property string rname: name
            property string rplace: place
            property string rday: day
        }
    }

    function row(kind, name) {
        for (var i = 0; i < rows.count; ++i) {
            var r = rows.itemAt(i)
            if (r && r.rkind === kind && (name === undefined || r.rname === name))
                return r
        }
        return null
    }

    function has(kind, name) {
        return row(kind, name) !== null
    }

    function hasItem(kind, item) {
        for (var i = 0; i < rows.count; ++i) {
            var r = rows.itemAt(i)
            if (r && r.rkind === kind && r.ritem === item)
                return true
        }
        return false
    }

    function favouriteNames() {
        var names = []
        for (var i = 0; i < rows.count; ++i) {
            var r = rows.itemAt(i)
            if (r && r.rkind === "favourite")
                names.push(r.rname)
        }
        return names.join(",")
    }

    function step(label, act, ok) {
        steps.push({ "label": label, "act": act, "ok": ok })
    }

    function tick() {
        var s = steps[stage]
        if (!s) {
            console.log("DONE")
            Qt.quit()
            return
        }
        if (!acted) {
            acted = true
            if (s.act)
                s.act()
        }
        if (s.ok()) {
            console.log("PASS " + s.label)
            stage++
            acted = false
            waited = 0
        } else if (++waited > 200) {
            console.log("FAILED: " + s.label)
            Qt.quit()
            stage = steps.length
        }
    }

    Timer {
        interval: 25
        repeat: true
        running: true
        onTriggered: root.tick()
    }

    Component.onCompleted: {
        Bridge.question.connect(function(questionId, kind, detailsJson) {
            root.questionKind = kind
            root.questionHost = JSON.parse(detailsJson).host || ""
            var answer = kind === "keyboard-interactive" ? { "responses": ["123456"] } : { "accept": true }
            Bridge.answer(questionId, JSON.stringify(answer))
        })
        Bridge.adhocConnected.connect(function(uri) { root.connected = uri })
        Bridge.failed.connect(function(kind, message, subject) {
            root.failedKind = kind
            root.failedSubject = subject
        })

        var docs = "lautta://user-documents/"
        var a = docs + "a.txt"
        var b = docs + "b.txt"

        step("consent row while the user has not decided",
             function() { locs.refresh() },
             function() { return Bridge.status === "consentUnknown" && has("consent") && !Bridge.ready })
        step("no account without consent", null, function() { return !has("account") && !has("adhocRecent") })
        step("ask again reaches the bridge",
             function() { Bridge.requestConsent() },
             function() { return Fake.consentRequests() >= 2 })
        step("granting shows the server",
             function() { Fake.grant() },
             function() {
                 var nas = row("account", "NAS")
                 return Bridge.ready && nas !== null && nas.rstatus === "ready" && nas.rprovider === "SFTP"
                        && nas.ruri === "lautta://nv-account:1/"
             })
        step("folders with visible item counts",
             null,
             function() {
                 var d = row("folder", "Documents")
                 return d !== null && d.rcount === 2 && has("folder", "Music") && has("deleted")
             })

        step("add a favourite",
             function() { favs.add(docs, "Docs", "#e7a33c") },
             function() {
                 var f = row("favourite", "Docs")
                 return f !== null && f.rplace === "Documents" && f.rcolour === "#e7a33c"
             })
        step("edit it",
             function() { favs.edit(favs.idOf(docs), "Papers", "#4fa3e5") },
             function() { return has("favourite", "Papers") && !has("favourite", "Docs") })
        step("order favourites",
             function() {
                 favs.add("lautta://user-music/", "Tunes", "")
             },
             function() { return favouriteNames() === "Papers,Tunes" })
        step("move one up",
             function() { favs.moveBy(favs.idOf("lautta://user-music/"), -1) },
             function() { return favouriteNames() === "Tunes,Papers" })
        step("remove favourites",
             function() {
                 favs.remove(favs.idOf(docs))
                 favs.remove(favs.idOf("lautta://user-music/"))
             },
             function() { return !has("favourite") })

        step("create a tag",
             function() { tagsModel.create("Work", "#e5604f") },
             function() { var t = row("tag", "Work"); return t !== null && t.rcount === 0 })
        step("assign tags and create another in one go",
             function() {
                 var id = parseInt(row("tag", "Work").ritem)
                 tagsModel.apply(JSON.stringify([a, b]), JSON.stringify([id]), "[]", "Taxes", "#4fa3e5")
             },
             function() {
                 var w = row("tag", "Work")
                 var t = row("tag", "Taxes")
                 return w !== null && w.rcount === 2 && t !== null && t.rcount === 2
             })
        step("tag page lists the items",
             function() { tagged.tagId = parseInt(row("tag", "Work").ritem) },
             function() { return tagged.count === 2 && tagged.missingCount === 0 && tagged.tagName === "Work" })
        step("a file removed outside shows up as missing",
             function() {
                 Fake.removeFile("Documents/b.txt")
                 var id = tagged.tagId
                 tagged.tagId = 0
                 tagged.tagId = id
             },
             function() { return tagged.count === 1 && tagged.missingCount === 1 })
        step("rename and recolour the tag",
             function() { tagged.rename("Job") },
             function() { return has("tag", "Job") && tagged.tagName === "Job" && !has("tag", "Work") })
        step("recolour the tag",
             function() { tagged.setColour("#9bd26a") },
             function() { var t = row("tag", "Job"); return t !== null && t.rcolour === "#9bd26a" })
        step("delete the tag",
             function() { tagged.deleteTag() },
             function() { return !has("tag", "Job") && root.removedSignals === 1 && has("tag", "Taxes") })

        step("recents come newest first with places",
             function() {
                 Fake.record(a, "a.txt", "opened")
                 Fake.record(docs + "c.txt", "c.txt", "edited")
                 Fake.record(docs + "d.zip", "d.zip", "transferred")
                 recents.reload()
             },
             function() {
                 return recents.loaded && recents.enabled && recents.count === 3 && recentRows.count === 3
                        && recentRows.itemAt(0).rname === "d.zip" && recentRows.itemAt(0).rplace === "Documents"
                        && recentRows.itemAt(0).rday === "today"
             })
        step("filter recents by kind",
             function() { recents.kindFilter = "edited" },
             function() { return recents.count === 1 && recentRows.itemAt(0).rname === "c.txt" })
        step("filter recents by text",
             function() { recents.kindFilter = ""; recents.text = "A.T" },
             function() { return recents.count === 1 && recentRows.itemAt(0).rname === "a.txt" })
        step("remove one recent",
             function() { recents.text = ""; recents.remove(recentRows.itemAt(0).rid) },
             function() { return recents.count === 2 })
        step("turn recents off through the setting",
             function() { App.setSetting("recents_enabled", JSON.stringify(false)); recents.reload() },
             function() { return !recents.enabled })
        step("clear recents",
             function() { recents.clear() },
             function() { return recents.count === 0 })

        step("attention shows on the server",
             function() { Fake.setAttention("account:1", "auth-failed") },
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rstatus === "attention" && nas.rattention === "auth-failed"
             })

        step("an ad-hoc server asks about its identity and connects",
             function() {
                 Fake.scriptQuestion("identity-unknown")
                 Bridge.connectAdHoc("sftp://host.example:2222/docs", "pw", JSON.stringify({ "user": "me" }))
             },
             function() {
                 return root.connected === "lautta://nv-adhoc:1/" && root.questionKind === "identity-unknown"
                        && root.questionHost === "host.example" && hasItem("adhoc", "nv-adhoc:1") && Fake.secrets() === '["pw"]'
                        && JSON.parse(Fake.answers())[0].accept === true
             })
        step("sign-in prompts are answered with the typed code",
             function() {
                 Fake.scriptQuestion("keyboard-interactive")
                 Bridge.connectAdHoc("sftp://other.example/", "", "{}")
             },
             function() {
                 var answers = JSON.parse(Fake.answers())
                 return root.questionKind === "keyboard-interactive" && answers.length === 2
                        && answers[1].answers[0] === "123456"
             })
        step("a refused connection says what and where",
             function() { Bridge.connectAdHoc("gopher://nowhere.example/", "", "{}") },
             function() { return root.failedKind === "Unsupported" && root.failedSubject === "nowhere.example" })
        step("forget an ad-hoc server",
             function() { Bridge.forgetAdHoc("nv-adhoc:1") },
             function() { return !hasItem("adhoc", "nv-adhoc:1") && !hasItem("adhocRecent", "nv-adhoc:1") })

        step("hand-offs reach the bridge",
             function() {
                 Bridge.addServer("")
                 Bridge.editAccount("lautta://nv-account:1/")
             },
             function() {
                 var h = JSON.parse(Fake.handoffs())
                 return h.indexOf("add:sftp") >= 0 && h.indexOf("settings:account:1") >= 0
             })

        step("location settings load",
             function() { prefs.locationId = "nv-account:1" },
             function() {
                 return prefs.loaded && prefs.locationName === "NAS" && prefs.kind === "account"
                        && prefs.provider === "SFTP" && prefs.bulkLanes === 0
             })
        step("location settings are saved and rename the row",
             function() {
                 prefs.displayName = "Home NAS"
                 prefs.bulkLanes = 3
                 prefs.noThumbCache = true
                 prefs.save()
             },
             function() { return has("account", "Home NAS") })

        step("an account removed in Settings is forgotten",
             function() {
                 favs.add("lautta://nv-account:1/photos", "Photos", "")
                 tagsModel.apply(JSON.stringify(["lautta://nv-account:1/photos/a.jpg"]), "[]", "[]", "Holiday", "#9bd26a")
             },
             function() { return has("favourite", "Photos") && row("tag", "Holiday").rcount === 1 })
        step("the account goes away",
             function() { Fake.removeAccount("account:1") },
             function() { return !has("account") && !has("favourite", "Photos") && row("tag", "Holiday").rcount === 0 })
    }
}
