// SPDX-License-Identifier: LGPL-2.1-or-later
.pragma library

var dayMs = 86400000

function _calendarDay(ms) {
    var d = new Date(ms)
    return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate())
}

function daysAgo(atMs, nowMs) {
    return (_calendarDay(nowMs) - _calendarDay(atMs)) / dayMs
}

function kind(atMs, nowMs) {
    var days = daysAgo(atMs, nowMs)
    if (days <= 0)
        return "today"
    if (days === 1)
        return "yesterday"
    if (days < 7)
        return "weekday"
    return "month"
}
