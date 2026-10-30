// SPDX-License-Identifier: LGPL-2.1-or-later
import QtQuick 2.6
import Sailfish.Silica 1.0
import "../../qml/components/browse"

ApplicationWindow {
    id: win

    property bool scenarioRunning: true
    property int waited: 0
    property int step: 0
    property real dimmest: 1.0
    readonly property var others: [on, off, failed, warning, none, locked]

    Column {
        width: 540

        RowStatusIcon { id: on; indicator: "on" }
        RowStatusIcon { id: off; indicator: "off" }
        RowStatusIcon { id: failed; indicator: "failed" }
        RowStatusIcon { id: warning; indicator: "warning" }
        RowStatusIcon { id: none; indicator: "" }
        RowStatusIcon { id: locked; indicator: "locked" }
        RowStatusIcon { id: busy; indicator: "busy" }
        Item {
            id: hidden
            visible: true
            RowStatusIcon { id: hiddenBusy; indicator: "busy"; appActive: true }
        }
    }

    function fail(what) {
        console.error("FAILED: " + what)
        scenarioRunning = false
    }

    function expect(ok, what) {
        if (!ok)
            console.error("FAILED: " + what)
        return ok
    }

    function dotOf(slot) {
        var kids = slot.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === "statusDot")
                return kids[i]
        }
        return null
    }

    function lockOf(slot) {
        var kids = slot.children
        for (var i = 0; i < kids.length; ++i) {
            if (kids[i].objectName === "statusLock")
                return kids[i]
        }
        return null
    }

    function checkLock() {
        var lock = lockOf(locked)
        if (!expect(lock !== null && lock.visible && locked.visible, "a locked drive shows no padlock"))
            return
        expect(String(lock.source) === "image://theme/icon-s-secure", "the padlock is " + lock.source)
        expect(!dotOf(locked).visible, "a locked drive shows a dot too")
        expect(lock.width > 0 && Math.round(lock.x + lock.width) === Math.round(locked.width),
               "the padlock ends at " + (lock.x + lock.width) + ", not at " + locked.width)
        expect(Math.round(lock.y + lock.height / 2) === Math.round(locked.height / 2), "the padlock is not centred")
        expect(String(lock.color) === String(Theme.primaryColor) && !lock.highlighted, "the padlock is " + lock.color)
        locked.highlighted = true
        expect(lock.highlighted, "a pressed row's padlock is not highlighted")
        locked.highlighted = false
        expect(!lockOf(on).visible && !lockOf(off).visible && !lockOf(none).visible, "a dot comes with a padlock")
        locked.indicator = "off"
        expect(!lock.visible && dotOf(locked).visible && locked.width === off.width, "an unlocked drive keeps its padlock")
        locked.indicator = "locked"
    }

    function pulseOf(slot) {
        var animations = slot.data
        for (var i = 0; i < animations.length; ++i) {
            if (animations[i].objectName === "statusPulse")
                return animations[i]
        }
        return null
    }

    function checkSameDot(name, slot) {
        var dot = dotOf(slot)
        var ref = dotOf(on)
        if (!expect(dot !== null && dot.visible, name + " shows no dot"))
            return
        expect(dot.width === ref.width && dot.height === ref.height, name + "'s dot is " + dot.width + "x" + dot.height
               + ", the on dot " + ref.width + "x" + ref.height)
        expect(dot.x === ref.x && dot.y === ref.y, name + "'s dot is at " + dot.x + "," + dot.y + ", the on dot at " + ref.x + "," + ref.y)
        expect(dot.radius === ref.radius && dot.falloffRadius === ref.falloffRadius, name + "'s dot glows otherwise than the on dot")
        expect(!dot.dimmed, name + "'s dot is dimmed")
        expect(Math.round(dot.x + (dot.width + slot.dotSize) / 2) === Math.round(slot.width),
               name + "'s dot ends at " + (dot.x + (dot.width + slot.dotSize) / 2) + ", not at " + slot.width)
    }

    function channels(hex) {
        return [1, 3, 5].map(function(i) { return parseInt(hex.substr(i, 2), 16) / 255 })
    }

    function hueOf(hex) {
        var c = channels(hex)
        var max = Math.max(c[0], c[1], c[2])
        var span = max - Math.min(c[0], c[1], c[2])
        if (span === 0)
            return -1
        var h = max === c[0] ? ((c[1] - c[2]) / span) % 6 : (max === c[1] ? (c[2] - c[0]) / span + 2 : (c[0] - c[1]) / span + 4)
        return Math.round(((h * 60) + 360) % 360)
    }

    function brightness(hex) {
        return Math.max.apply(null, channels(hex))
    }

    function colourOf(slot) {
        return String(dotOf(slot).color)
    }

    function checkColours() {
        var want = [["on", on, Theme.highlightColor], ["off", off, Theme.primaryColor],
                    ["busy", busy, Theme.primaryColor], ["failed", failed, Theme.errorColor]]
        for (var i = 0; i < want.length; ++i) {
            expect(colourOf(want[i][1]) === String(want[i][2]),
                   want[i][0] + "'s dot is " + colourOf(want[i][1]) + ", not " + want[i][2])
        }
        var orange = String(dotOf(warning).color)
        var hue = hueOf(orange)
        expect(hue > 20 && hue < 45 && brightness(orange) > 0.8, "the warning dot is " + orange + ", not orange (hue " + hue + ")")
        var own = [Theme.highlightColor, Theme.primaryColor, Theme.errorColor, Theme.secondaryColor,
                   Theme.highlightBackgroundColor, Theme.secondaryHighlightColor]
        for (var j = 0; j < own.length; ++j)
            expect(orange !== String(own[j]), "the warning dot is the theme's " + own[j])
        var looks = [on, off, failed, warning].map(colourOf)
        for (var a = 0; a < looks.length; ++a) {
            for (var b = a + 1; b < looks.length; ++b)
                expect(looks[a] !== looks[b], "two states are both " + looks[a])
        }
        expect(none.visible === false && dotOf(none).visible === false, "a row without a state shows a dot")
    }

    function checkHighlighted() {
        var slots = [[on, Theme.highlightColor], [off, Theme.highlightColor], [busy, Theme.highlightColor],
                     [failed, Theme.errorColor], [warning, warning.warningColor]]
        for (var i = 0; i < slots.length; ++i)
            slots[i][0].highlighted = true
        for (var j = 0; j < slots.length; ++j) {
            expect(colourOf(slots[j][0]) === String(slots[j][1]),
                   "a pressed " + slots[j][0].indicator + " dot is " + colourOf(slots[j][0]) + ", not " + slots[j][1])
            slots[j][0].highlighted = false
        }
        expect(colourOf(off) === String(Theme.primaryColor), "the off dot keeps its colour once released")
    }

    function checkOthersStill() {
        for (var i = 0; i < others.length; ++i) {
            expect(!others[i].pulsing && !pulseOf(others[i]).running && dotOf(others[i]).opacity === 1.0,
                   (others[i].indicator || "empty") + " pulses")
        }
    }

    function checkStill() {
        checkOthersStill()
        expect(Qt.application.state !== Qt.ApplicationActive && !busy.appActive,
               "the app is in front on the target: " + Qt.application.state)
        expect(!busy.pulsing && !pulseOf(busy).running && dotOf(busy).opacity === 1.0, "a busy dot pulses in the background")
    }

    Timer {
        interval: 50
        repeat: true
        running: win.scenarioRunning
        onTriggered: {
            if (++win.waited > 200)
                return win.fail("step " + win.step + " timed out")
            switch (win.step) {
            case 0:
                win.checkSameDot("on", on)
                win.checkSameDot("off", off)
                win.checkSameDot("busy", busy)
                win.checkSameDot("failed", failed)
                win.checkSameDot("warning", warning)
                win.checkColours()
                win.checkHighlighted()
                win.checkLock()
                win.checkStill()
                for (var k = 0; k < win.others.length; ++k)
                    win.others[k].appActive = true
                busy.appActive = true
                break
            case 1:
                if (!win.expect(busy.pulsing && pulseOf(busy).running, "a busy dot does not pulse with the app in front"))
                    return
                win.dimmest = Math.min(win.dimmest, dotOf(busy).opacity)
                if (win.dimmest > Theme.opacityLow + 0.2)
                    return
                win.expect(colourOf(busy) === String(Theme.primaryColor), "the busy dot changed colour")
                win.checkOthersStill()
                busy.indicator = "on"
                break
            case 2:
                win.expect(!busy.pulsing && !pulseOf(busy).running, "the pulse runs after the row stopped being busy")
                win.expect(dotOf(busy).opacity === 1.0, "the dot is left at " + dotOf(busy).opacity + " after the pulse")
                win.expect(colourOf(busy) === String(Theme.highlightColor), "the connected dot is " + colourOf(busy))
                busy.indicator = "busy"
                win.dimmest = 1.0
                break
            case 3:
                if (!win.expect(busy.pulsing && pulseOf(busy).running, "a busy dot does not pulse again"))
                    return
                win.dimmest = Math.min(win.dimmest, dotOf(busy).opacity)
                if (win.dimmest > Theme.opacityLow + 0.2)
                    return
                win.expect(hiddenBusy.pulsing && pulseOf(hiddenBusy).running, "a busy dot on a page that is shown does not pulse")
                hidden.visible = false
                break
            case 4:
                win.expect(!hiddenBusy.visible && !hiddenBusy.pulsing && !pulseOf(hiddenBusy).running,
                           "the pulse runs on a page that is not shown")
                win.expect(dotOf(hiddenBusy).opacity === 1.0, "the dot is left at " + dotOf(hiddenBusy).opacity + " when its page is covered")
                busy.appActive = false
                break
            case 5:
                win.expect(!busy.pulsing && !pulseOf(busy).running && dotOf(busy).opacity === 1.0,
                           "the pulse runs with the app in the background")
                win.scenarioRunning = false
                break
            }
            win.step++
            win.waited = 0
        }
    }
}
