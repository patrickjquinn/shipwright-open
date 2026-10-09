// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// What Pilot may know when the person asks about the room page (Keel Actions,
// core/src/actions.rs): which conversation it is, its name and its last few
// messages, so that "reply to this" can be answered. The text is built only
// while the page is on top (`active`, bound by the page). An invitation gives
// its name only: nothing in it has been accepted yet.
//
// A file of its own because core/build.rs reads every QML file that imports
// Keel.Actions, and its reader does not take everything RoomPage.qml holds.
import QtQuick 2.0
import Keel.Actions 1.0

KeelContext {
    property string roomId
    property string roomName
    property bool invited

    purpose: "reading a conversation"
    entity: roomId.length > 0 && !invited
            ? "keel://org.shipwright.ShoalMessages/conversation/" + roomId : ""
    text: active && !invited && matrix.timelineReady
          ? roomName + "\n" + matrix.timeline.transcript : roomName
}
