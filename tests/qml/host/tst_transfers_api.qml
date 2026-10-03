// SPDX-License-Identifier: LGPL-2.1-or-later
// Host test of the Transfers singleton and models (no Silica): the
// synchronous behaviour of doc/QML-API.md on an empty queue. The queue
// itself (copy, conflicts, history) is exercised by
// crates/lautta-qt/tests/transfers_flow.rs.
import QtQuick 2.6
import Lautta 1.0

Item {
    id: root

    property string lastKind: ""
    property int failedCount: 0
    property int showRequests: 0

    function check(cond, what) {
        if (!cond)
            console.error("FAILED: " + what)
    }

    TransfersModel { id: model }
    Repeater {
        model: model
        delegate: Item { }
    }
    TransferItemsModel {
        id: items

        transferId: 4242
    }
    WorkingCopiesModel { id: copies }
    Repeater {
        model: copies
        delegate: Item { }
    }

    Component.onCompleted: {
        // connect() instead of Connections: Qt 5.6 on the target and Qt 5.15
        // here disagree about the onFoo syntax.
        Transfers.failed.connect(function(kind, message) {
            root.lastKind = kind
            root.failedCount += 1
        })
        Transfers.showRequested.connect(function() { root.showRequests += 1 })
        check(Transfers.activeCount === 0, "no active transfers")
        check(Transfers.pendingCount === 0 && Transfers.pausedCount === 0 && Transfers.waitingCount === 0, "no pending transfers")
        check(!Transfers.busy, "not busy")
        check(Transfers.bytesDone === 0 && Transfers.bytesTotal === 0 && Transfers.rate === 0, "no progress")
        check(Transfers.eta === -1, "unknown ETA is -1, got " + Transfers.eta)
        check(Transfers.pendingAtStart === 0 && !Transfers.closing, "nothing restored, not closing")

        var before = failedCount
        check(!Transfers.pause(987654), "pausing an unknown transfer fails")
        check(failedCount === before + 1 && lastKind === "NotFound", "pause reports NotFound, got " + lastKind)
        check(!Transfers.resume(987654), "resuming an unknown transfer fails")
        check(!Transfers.cancel(987654), "canceling an unknown transfer fails")
        check(!Transfers.retryFailed(987654), "retrying an unknown transfer fails")
        check(!Transfers.moveUp(987654) && !Transfers.moveDown(987654) && !Transfers.moveToTop(987654), "reordering an unknown transfer is refused")

        before = failedCount
        check(!Transfers.answer(5, 0, "obliterate", false), "unknown choices are refused")
        check(failedCount === before + 1 && lastKind === "InvalidArgument", "bad choice reports InvalidArgument, got " + lastKind)
        check(!Transfers.answer(5, -1, "skip", false), "negative items are refused")
        check(lastKind === "InvalidArgument", "bad item reports InvalidArgument")
        check(!Transfers.answer(987654, 0, "skip", false), "answering an unknown transfer fails")
        check(lastKind === "NotFound", "unknown transfer reports NotFound, got " + lastKind)

        before = failedCount
        Transfers.queueCopy("[]", "not a uri")
        check(failedCount === before + 1 && lastKind === "InvalidArgument", "bad destination reports InvalidArgument")
        before = failedCount
        Transfers.resolveEditConflict(1, "bogus")
        check(failedCount === before + 1 && lastKind === "InvalidArgument", "bad edit choice reports InvalidArgument")

        check(Transfers.questionsJson(5) === "[]", "no questions for an unknown transfer")
        check(Transfers.summaryJson(5) === "", "no summary for an unknown transfer")
        check(Transfers.clearHistory() === 0, "nothing to clear")

        Transfers.requestShow()
        check(showRequests === 1, "requestShow asks for the page")
        Transfers.noteClosing()
        check(Transfers.closing, "closing is noted")
        Transfers.dismissRestored()
        check(Transfers.pendingAtStart === 0, "dismissing without a prompt changes nothing")

        check(model.count === 0, "the list starts empty")
        model.refresh()
        check(model.count === 0, "still empty after a refresh")
        check(items.transferId === 4242, "the items model keeps its transfer id")
        check(items.count === 0 && items.summaryJson === "", "an unknown transfer has no items and no summary")
        items.transferId = 4243
        check(items.transferId === 4243, "the transfer id can change")
        check(copies.count === 0, "no working copies yet")
    }
}
