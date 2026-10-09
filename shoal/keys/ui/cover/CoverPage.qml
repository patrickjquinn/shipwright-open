// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Cover: lock state and the number of entries, never entry names or codes
// (the cover is visible on the home screen to anyone holding the phone).
// Unlocked, the cover actions lock the vault and search it.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

CoverBackground {
    id: cover

    // Cover action "Search": the window opens the entry list with the
    // search field focused (shipwright-shoal-keys.qml).
    signal searchRequested()

    readonly property bool unlocked: ShoalKeys.vaultExists && !ShoalKeys.locked

    Column {
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            topMargin: Theme.paddingLarge * 2
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingMedium

        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.iconSizeLarge
            height: Theme.iconSizeLarge
            source: cover.unlocked ? "image://theme/icon-m-keys" : "image://theme/icon-m-device-lock"
        }
        Label {
            objectName: "coverState"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeLarge
            text: ShoalKeys.busy ? qsTr("Working…")
                : !ShoalKeys.vaultExists ? qsTr("No vault")
                : ShoalKeys.locked ? qsTr("Locked")
                : qsTr("Unlocked")
        }
        Label {
            objectName: "coverDetail"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.secondaryColor
            visible: text.length > 0
            text: cover.unlocked ? qsTr("%n (entry|entries)", "", ShoalKeys.entryCount) : ""
        }
    }

    CoverActionList {
        objectName: "coverActions"
        enabled: cover.unlocked
        CoverAction {
            // No icon-cover-lock exists in sailfish-default 5.2; the monochrome
            // device-lock icon is used instead.
            iconSource: "image://theme/icon-m-device-lock"
            onTriggered: ShoalKeys.lock()
        }
        CoverAction {
            iconSource: "image://theme/icon-cover-search"
            onTriggered: cover.searchRequested()
        }
    }
}
