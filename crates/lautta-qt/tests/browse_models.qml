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
    property bool relisted: false
    property bool moved: false
    property bool movedAtOnce: false
    property bool refused: false

    LocationsModel { id: locs }
    FavouritesModel { id: favs }
    RecentsModel { id: recents }
    DirectoryModel { id: remoteDir }
    DirectoryModel { id: localDir }

    // The thumbnail source of each row of the remote folder.
    Repeater {
        id: remoteRows
        model: remoteDir
        delegate: Item {
            property string rname: name
            property string rthumb: thumbnailSource
        }
    }

    Repeater {
        id: rows
        model: locs
        delegate: Item {
            property string rkind: kind
            property string rname: name
            property string rstatus: status
            property string rattention: attention
            property string rconnection: connection
            property string rconnectionError: connectionError
            property string rcolour: colour
            property string rplace: place
            property string rprovider: provider
            property string ruri: uri
            property string ritem: itemId
            property string rsection: section
            property string ricon: icon
            property bool rfile: isFile
            property string rdrive: drive
            // Set by a step to tell whether this delegate survived a change.
            property bool marked: false
        }
    }

    Repeater {
        id: recentRows
        model: recents
        delegate: Item {
            property int rid: itemId
            property string rname: name
            property string rplace: place
            property string rgroup: dateGroup
        }
    }

    // Settings › Browse page (BRW-12).
    BrowseLayoutModel { id: layout }

    Repeater {
        id: layoutRows
        model: layout
        delegate: Item {
            property string lsection: section
            property string litem: item
            property bool lhidden: hidden
            property int lplace: place
            property bool lmovable: movable
            property bool lnested: nested
            // Set by a step to tell whether this delegate survived a change.
            property bool marked: false
        }
    }

    function layoutRow(section, item) {
        for (var i = 0; i < layoutRows.count; ++i) {
            var r = layoutRows.itemAt(i)
            if (r && r.lsection === section && r.litem === item)
                return r
        }
        return null
    }

    // The list row of a layout row, -1 when none.
    function layoutIndex(section, item) {
        for (var i = 0; i < layoutRows.count; ++i) {
            var r = layoutRows.itemAt(i)
            if (r && r.lsection === section && r.litem === item)
                return i
        }
        return -1
    }

    // The items of a section in the settings list, in order.
    function layoutKeys(section) {
        var items = []
        for (var i = 0; i < layoutRows.count; ++i) {
            var r = layoutRows.itemAt(i)
            if (r && r.lsection === section)
                items.push(r.litem)
        }
        return items
    }

    function deviceRows() {
        var names = []
        for (var i = 0; i < rows.count; ++i) {
            var r = rows.itemAt(i)
            if (r && r.rsection === "device")
                names.push(r.rkind === "deleted" || r.rkind === "recents" ? r.rkind : r.rname)
        }
        return names.join(",")
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

    // What the remote folder gives the file `name` as thumbnail, null without it.
    function thumbOf(name) {
        for (var i = 0; i < remoteRows.count; ++i) {
            if (remoteRows.itemAt(i).rname === name)
                return remoteRows.itemAt(i).rthumb
        }
        return null
    }

    function step(label, act, ok) {
        steps.push({ "label": label, "act": act, "ok": ok })
    }

    function tick() {
        var s = steps[stage]
        if (!s) {
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
            console.error("FAILED: " + s.label)
            stage = steps.length
        }
    }

    // Runs the steps one after the other, letting Qt work in between.
    function run() {
        while (stage < steps.length) {
            var before = stage
            tick()
            if (stage === before)
                FakeWorld.pump(25)
        }
        console.log("DONE")
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

        step("consent row while the user has not decided",
             function() { locs.refresh() },
             function() { return Bridge.status === "consentUnknown" && has("consent") && !Bridge.ready })
        step("Settings › Storage opens without the consent (NVB-13)",
             function() { if (Bridge.storageSettings) Bridge.openStorageSettings() },
             function() { return JSON.parse(FakeWorld.handoffs()).indexOf("storage") >= 0 })
        // A drive is mounted (where the sandbox does not see it here) and
        // unmounted without the consent (LOC-3).
        step("a drive that is not mounted is a volume row",
             null,
             function() {
                 var v = row("volume", "STICK")
                 return v !== null && v.rstatus === "unmounted" && v.rdrive === "sda1" && v.ruri === ""
                        && v.ritem === "vol-STICK" && v.ricon === "image://theme/icon-m-usb"
             })
        // Mounted where the sandbox does not see it, it is no row at all.
        step("a drive mounted out of the app's sight is an error naming it",
             function() { Bridge.mountDrive("sda1", "STICK") },
             function() {
                 return root.failedKind === "NotFound" && root.failedSubject === "STICK"
                        && JSON.parse(FakeWorld.drives())[0].mounted && !has("volume", "STICK")
             })
        step("unmounting reaches the bridge and the row comes back",
             function() {
                 root.failedKind = ""
                 Bridge.unmountDrive("sda1", "STICK")
             },
             function() {
                 var v = row("volume", "STICK")
                 return !JSON.parse(FakeWorld.drives())[0].mounted && v !== null && v.rstatus === "unmounted"
                        && root.failedKind === "" && root.connected === ""
             })
        step("no account without consent", null, function() { return !has("account") && !has("adhocRecent") })
        step("ask again reaches the bridge",
             function() { Bridge.requestConsent() },
             function() { return FakeWorld.consentRequests() >= 2 })
        step("granting shows the server",
             function() { FakeWorld.grant() },
             function() {
                 var nas = row("account", "NAS")
                 return Bridge.ready && nas !== null && nas.rstatus === "ready" && nas.rprovider === "SFTP"
                        && nas.ruri === "lautta://nv-account:1/"
             })
        step("an unused server is not connected",
             null,
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rconnection === "disconnected" && nas.rconnectionError === ""
             })
        step("folders, Recents and Trash are rows",
             null,
             function() { return has("folder", "Documents") && has("folder", "Music") && has("recents") && has("deleted") })

        step("Browse page settings list every row",
             function() { layout.reload() },
             function() {
                 var m = layoutRow("device", "user-music")
                 var n = layoutRow("nearby", "nearby")
                 return m !== null && !m.lhidden && m.lmovable && n !== null && !n.lhidden && !n.lmovable
                        && layoutRow("device", "deleted") !== null
             })
        step("hide a folder in the settings",
             function() { layout.setHidden("device", "user-music", true) },
             function() {
                 var m = layoutRow("device", "user-music")
                 return !has("folder", "Music") && has("folder", "Documents") && m !== null && m.lhidden
             })
        // Settings › Browse page follows what the Browse page follows: a
        // drive that comes shows in both (BRW-12, LOC-3).
        step("a drive that comes shows in the Browse page and its settings",
             function() { FakeWorld.addDrive("mmcblk1p1", "CARD") },
             function() {
                 var v = row("volume", "CARD")
                 var l = layoutRow("volumes", "vol-CARD")
                 return v !== null && v.rdrive === "mmcblk1p1" && l !== null && !l.lhidden
             })
        // The settings page keeps its rows (and the user's place in the
        // list) on a switch and a move: no row is rebuilt.
        step("show it again from the settings",
             function() {
                 layoutRow("device", "user-music").marked = true
                 layoutRow("device", "deleted").marked = true
                 layout.setHidden("device", "user-music", false)
             },
             function() {
                 var m = layoutRow("device", "user-music")
                 return has("folder", "Music") && m !== null && !m.lhidden && m.marked
             })
        step("move a row within its section",
             function() { layout.moveTo("device", "deleted", 0) },
             function() {
                 var d = layoutRow("device", "deleted")
                 return deviceRows().indexOf("deleted,recents,Documents") === 0 && d !== null && d.lplace === 0
                        && d.marked && layoutRow("device", "user-music").marked
             })
        step("restore defaults",
             function() {
                 layout.setHidden("device", "user-documents", true)
                 layout.restoreDefaults()
             },
             function() {
                 var d = layoutRow("device", "deleted")
                 return deviceRows().indexOf("recents,Documents") === 0 && /,deleted$/.test(deviceRows())
                        && d !== null && d.lplace > 0
             })
        // A dragged row follows the finger (BRW-12): it moves at once, by
        // the same delegate, the places follow, and the order is stored
        // behind it without the row moving back.
        step("a carried row moves at once and is stored",
             function() {
                 var at = layoutIndex("device", "recents")
                 var nearby = layoutIndex("nearby", "nearby")
                 layoutRow("device", "recents").marked = true
                 root.moved = layout.moveRow(at, at + 1) && layout.moveRow(at + 1, at + 2)
                 var r = layoutRows.itemAt(at + 2)
                 root.movedAtOnce = r.litem === "recents" && r.marked && r.lplace === 2
                                    && layoutRows.itemAt(at).lplace === 0
                 root.refused = !layout.moveRow(at, at) && !layout.moveRow(nearby, nearby - 1)
                                && !layout.moveRow(at, layoutRows.count)
             },
             function() {
                 var r = layoutRow("device", "recents")
                 return root.moved && root.movedAtOnce && root.refused
                        && deviceRows().split(",")[2] === "recents" && r.lplace === 2 && r.marked
             })
        // Volumes and Servers lead with a switch each; off, the section's
        // rows leave the settings list and the Browse page (and so the
        // preview, which is the Browse page's model) and come back as they
        // were (BRW-12). Rows change in place, no model reset.
        step("Volumes and Servers lead with their switch",
             function() { layout.reload() },
             function() {
                 var v = layoutRow("volumes", "section")
                 var s = layoutRow("servers", "section")
                 var card = layoutRow("volumes", "vol-CARD")
                 var nas = layoutRow("servers", "nv-account:1")
                 return v !== null && s !== null && !v.lhidden && !s.lhidden && v.lplace === -1 && s.lplace === -1
                        && !v.lmovable && !v.lnested && card !== null && card.lnested && nas !== null && nas.lnested
                        && layoutIndex("volumes", "section") + 1 === layoutIndex("volumes", "vol-CARD")
                        && layoutIndex("servers", "section") + 1 === layoutIndex("servers", "nv-account:1")
                        && layoutRow("device", "recents").lnested === false
             })
        step("Servers off hides its rows, not the other sections",
             function() {
                 layout.setHidden("servers", "nv-account:1", true)
                 layoutRow("servers", "section").marked = true
                 layoutRow("volumes", "section").marked = true
                 layoutRow("volumes", "vol-CARD").marked = true
                 layout.setHidden("servers", "section", true)
             },
             function() {
                 var s = layoutRow("servers", "section")
                 return s !== null && s.lhidden && s.marked && layoutRow("servers", "nv-account:1") === null
                        && !has("account") && !has("adhocRecent")
                        && layoutRow("volumes", "section").marked && layoutRow("volumes", "vol-CARD").marked
                        && has("volume", "CARD") && has("folder", "Music")
             })
        step("a server that comes while Servers is off shows nowhere",
             function() { FakeWorld.addAccount(2, "Office") },
             function() {
                 return FakeWorld.locationKnown("nv-account:2") && layoutRow("servers", "section").lhidden
                        && !has("account") && layoutRow("servers", "nv-account:2") === null
                        && JSON.stringify(layoutKeys("servers")) === '["section"]'
             })
        step("Servers on shows its rows again with their earlier switches",
             function() {
                 layoutRow("servers", "section").marked = true
                 layout.setHidden("servers", "section", false)
             },
             function() {
                 var s = layoutRow("servers", "section")
                 var nas = layoutRow("servers", "nv-account:1")
                 var office = layoutRow("servers", "nv-account:2")
                 return s !== null && !s.lhidden && s.marked && nas !== null && nas.lhidden
                        && office !== null && !office.lhidden && has("account", "Office") && !has("account", "NAS")
             })
        step("Volumes off removes the volumes from the Browse page",
             function() { layout.setHidden("volumes", "section", true) },
             function() {
                 var v = layoutRow("volumes", "section")
                 return v !== null && v.lhidden && layoutRow("volumes", "vol-CARD") === null
                        && !has("volume") && has("account", "Office")
                        && JSON.stringify(layoutKeys("volumes")) === '["section"]'
             })
        step("restore defaults switches both sections on and shows every row",
             function() { layout.restoreDefaults() },
             function() {
                 return !layoutRow("volumes", "section").lhidden && !layoutRow("servers", "section").lhidden
                        && has("volume", "CARD") && has("account", "NAS") && has("account", "Office")
                        && !layoutRow("servers", "nv-account:1").lhidden
             })
        step("the second server goes away",
             function() { FakeWorld.removeAccount("account:2") },
             function() { return !has("account", "Office") && layoutRow("servers", "nv-account:2") === null })

        step("a remote folder lists",
             function() {
                 FakeWorld.putFile("account:1", "r1.txt")
                 remoteDir.uri = "lautta://nv-account:1/"
             },
             function() { return !remoteDir.loading && remoteDir.count === 1 })
        step("coming back to a remote folder lists it again",
             function() {
                 FakeWorld.putFile("account:1", "r2.txt")
                 remoteDir.revisit()
                 root.relisted = remoteDir.loading
             },
             function() { return root.relisted && !remoteDir.loading && remoteDir.count === 2 })
        // A server's pictures and PDFs are made by the app, with the switch
        // of servers (PRV-2, PRV-14); an open folder follows it at once.
        step("a server's pictures and PDFs have the app's thumbnails",
             function() {
                 FakeWorld.putFile("account:1", "p.jpg")
                 FakeWorld.putFile("account:1", "d.pdf")
                 remoteDir.revisit()
             },
             function() {
                 var base = "image://lautta-thumb/lautta://nv-account:1/"
                 return !remoteDir.loading && remoteDir.count === 4 && thumbOf("p.jpg") === base + "p.jpg"
                        && thumbOf("d.pdf") === base + "d.pdf" && thumbOf("r1.txt") === ""
             })
        step("Thumbnails from servers off takes them from the open folder",
             function() { App.setSetting("thumbnails_remote", JSON.stringify(false)) },
             function() { return thumbOf("p.jpg") === "" && thumbOf("d.pdf") === "" })
        step("and on again gives them back",
             function() { App.setSetting("thumbnails_remote", JSON.stringify(true)) },
             function() { return thumbOf("p.jpg") !== "" && thumbOf("d.pdf") !== "" })
        step("listing connected the server",
             null,
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rconnection === "connected"
             })
        // A state change is a change of the row, not a new row (NVB-4).
        step("a failed connection updates the server row in place",
             function() {
                 row("account", "NAS").marked = true
                 FakeWorld.setState("account:1", "failed", "Timeout")
             },
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rconnection === "failed" && nas.rconnectionError === "Timeout"
                        && nas.rstatus === "ready" && nas.marked
             })
        step("and so does closing it when idle",
             function() { FakeWorld.setState("account:1", "disconnected", "") },
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rconnection === "disconnected" && nas.rconnectionError === ""
                        && nas.marked
             })
        step("a local folder lists",
             function() { localDir.uri = docs },
             function() { return !localDir.loading && localDir.count === 1 })
        step("coming back to a watched local folder does not list it again",
             function() {
                 localDir.revisit()
                 root.relisted = localDir.loading
             },
             function() { return !root.relisted })

        step("add a favourite",
             function() { favs.add(docs, "Docs", "#e7a33c") },
             function() {
                 var f = row("favourite", "Docs")
                 return f !== null && f.rplace === "Documents" && f.rcolour === "#e7a33c"
             })
        step("the settings list shows it",
             null,
             function() { return layoutRow("favourites", String(favs.idOf(docs))) !== null })
        step("edit it",
             function() { favs.edit(favs.idOf(docs), "Papers", "#4fa3e5") },
             function() { return has("favourite", "Papers") && !has("favourite", "Docs") })
        // A row remorse lives in its delegate: a change elsewhere in the
        // list must not rebuild the rows that stay.
        step("order favourites, keeping the other rows",
             function() {
                 row("favourite", "Papers").marked = true
                 row("folder", "Music").marked = true
                 favs.add("lautta://user-music/", "Tunes", "")
             },
             function() {
                 return favouriteNames() === "Papers,Tunes" && row("favourite", "Papers").marked
                        && row("folder", "Music").marked
             })
        step("move one up",
             function() { favs.moveTo(favs.idOf("lautta://user-music/"), 0) },
             function() { return favouriteNames() === "Tunes,Papers" })
        step("remove favourites",
             function() {
                 favs.remove(favs.idOf(docs))
                 favs.remove(favs.idOf("lautta://user-music/"))
             },
             function() { return !has("favourite") })
        // The favourite page: opened on a folder, it saves by URI, adds
        // a second favourite next to the first and removes by URI.
        step("page adds a first favourite",
             function() { favs.save(docs, "Docs", "", "", true) },
             function() { return favouriteNames() === "Docs" })
        step("page adds a second favourite",
             function() { favs.save("lautta://user-music/", "Tunes", "#4fa3e5", "", true) },
             function() { return favouriteNames() === "Docs,Tunes" })
        step("page relabels a favourite",
             function() { favs.save(docs, "Papers", "", "", true) },
             function() { return favouriteNames() === "Papers,Tunes" })
        // ORG-1: the chosen icon is stored and shown, tinted; an unknown one
        // is the default star.
        step("page gives a favourite an icon",
             function() { favs.save(docs, "Papers", "#e5604f", "home", true) },
             function() {
                 var f = row("favourite", "Papers")
                 var stored = JSON.parse(favs.get(favs.idOf(docs)))
                 return f !== null && f.ricon === "image://theme/icon-m-home" && f.rcolour === "#e5604f"
                        && stored.icon === "home" && stored.defaultIcon === "image://theme/icon-m-favorite"
                        && favs.iconChoices()[0] === "favorite" && favs.iconChoices().indexOf("home") > 0
             })
        step("an unknown icon is the star",
             function() { favs.save(docs, "Papers", "", "no-such-icon", true) },
             function() {
                 var f = row("favourite", "Papers")
                 return f !== null && f.ricon === "image://theme/icon-m-favorite" && f.rcolour === ""
                        && JSON.parse(favs.get(favs.idOf(docs))).icon === ""
             })
        // ORG-1: a file is a favourite as a file, under the folder it is in.
        step("page adds a file favourite",
             function() { favs.save(a, "Notes", "", "", false) },
             function() {
                 var f = row("favourite", "Notes")
                 var stored = JSON.parse(favs.get(favs.idOf(a)))
                 return f !== null && f.rfile && f.rplace === "Documents" && f.ricon === "image://theme/icon-m-document"
                        && stored.isDir === false && stored.place === "Documents" && stored.icon === ""
                        && stored.defaultIcon === "image://theme/icon-m-document"
                        && !row("favourite", "Papers").rfile && row("favourite", "Papers").ricon === "image://theme/icon-m-favorite"
             })
        step("relabelling it keeps it a file",
             function() { favs.save(a, "Lecture notes", "", "", true) },
             function() {
                 var f = row("favourite", "Lecture notes")
                 return f !== null && f.rfile && JSON.parse(favs.get(favs.idOf(a))).isDir === false
             })
        step("page removes favourites",
             function() {
                 favs.removeUri(docs)
                 favs.removeUri("lautta://user-music/")
                 favs.removeUri(a)
             },
             function() { return !has("favourite") && favs.idOf(docs) === -1 && favs.idOf(a) === -1 })
        step("a closed page's change still reaches Browse",
             function() {
                 var gone = Qt.createQmlObject("import Lautta 1.0; FavouritesModel { }", root)
                 gone.save(docs, "Gone", "", "", true)
                 gone.destroy()
             },
             function() { return favouriteNames() === "Gone" })
        step("and its removal too",
             function() {
                 var gone = Qt.createQmlObject("import Lautta 1.0; FavouritesModel { }", root)
                 gone.removeUri(docs)
                 gone.destroy()
             },
             function() { return !has("favourite") })

        step("recents come newest first with places",
             function() {
                 FakeWorld.record(a, "a.txt", "opened")
                 FakeWorld.record(docs + "c.txt", "c.txt", "previewed")
                 FakeWorld.record(docs + "d.zip", "d.zip", "transferred")
                 recents.reload()
             },
             function() {
                 return recents.loaded && recents.enabled && recents.count === 3 && recentRows.count === 3
                        && recentRows.itemAt(0).rname === "d.zip" && recentRows.itemAt(0).rplace === "Documents"
                        && recentRows.itemAt(0).rgroup === "today"
             })
        step("remove one recent",
             function() { recents.remove(recentRows.itemAt(0).rid) },
             function() { return recents.count === 2 && recentRows.itemAt(0).rname === "c.txt" })
        step("turn recents off through the setting",
             function() { App.setSetting("recents_enabled", JSON.stringify(false)); recents.reload() },
             function() { return !recents.enabled })
        step("clear recents",
             function() { recents.clear() },
             function() { return recents.count === 0 })

        step("attention shows on the server",
             function() { FakeWorld.setAttention("account:1", "auth-failed") },
             function() {
                 var nas = row("account", "NAS")
                 return nas !== null && nas.rstatus === "attention" && nas.rattention === "auth-failed"
             })

        step("an ad-hoc server asks about its identity and connects",
             function() {
                 FakeWorld.scriptQuestion("identity-unknown")
                 Bridge.connectAdHoc("sftp://host.example:2222/docs", "pw", JSON.stringify({ "user": "me" }))
             },
             function() {
                 return root.connected === "lautta://nv-adhoc:1/" && root.questionKind === "identity-unknown"
                        && root.questionHost === "host.example" && hasItem("adhoc", "nv-adhoc:1") && FakeWorld.secrets() === '["pw"]'
                        && JSON.parse(FakeWorld.answers())[0].accept === true
             })
        step("sign-in prompts are answered with the typed code",
             function() {
                 FakeWorld.scriptQuestion("keyboard-interactive")
                 Bridge.connectAdHoc("sftp://other.example/", "", "{}")
             },
             function() {
                 var answers = JSON.parse(FakeWorld.answers())
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
                 var h = JSON.parse(FakeWorld.handoffs())
                 return h.indexOf("add:sftp") >= 0 && h.indexOf("settings:account:1") >= 0
             })

        step("an account removed in Settings is forgotten",
             function() { favs.add("lautta://nv-account:1/photos", "Photos", "") },
             function() { return has("favourite", "Photos") })
        step("the account goes away",
             function() { FakeWorld.removeAccount("account:1") },
             function() { return !has("account") && !has("favourite", "Photos") })
        run()
    }
}
