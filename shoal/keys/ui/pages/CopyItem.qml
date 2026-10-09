// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// A label and value; tap copies the value (every copy is cleared from the
// clipboard on the Settings timer). Secret values show as dots; with
// `revealable` an eye button at the right shows and hides them.

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Keys 1.0

ListItem {
    id: item

    property string label
    property string value
    property string copyValue: value
    property bool secret: false
    property bool revealable: false
    // Monospace for values whose characters matter (passwords, codes, URLs).
    property bool monospace: true
    // Room kept at the right for an item the owner places there.
    property real trailingWidth: 0
    property bool _copied: false

    // The eye button was pressed: the owner decides what `secret` becomes.
    signal revealToggled()

    visible: value.length > 0
    contentHeight: Math.max(Theme.itemSizeMedium, column.height + 2 * Theme.paddingMedium)
    onClicked: {
        ShoalKeys.copy(copyValue)
        _copied = true
        copiedTimer.restart()
    }

    Timer {
        id: copiedTimer
        interval: 1500
        onTriggered: item._copied = false
    }

    Column {
        id: column
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: reveal.visible ? reveal.left : parent.right
            rightMargin: (reveal.visible ? Theme.paddingSmall : Theme.horizontalPageMargin) + item.trailingWidth
            verticalCenter: parent.verticalCenter
        }
        Label {
            width: parent.width
            text: item._copied ? qsTr("Copied") : item.label
            textFormat: Text.PlainText
            truncationMode: TruncationMode.Fade
            font.pixelSize: Theme.fontSizeExtraSmall
            color: item.highlighted || item._copied ? Theme.secondaryHighlightColor : Theme.secondaryColor
        }
        Label {
            width: parent.width
            text: item.secret ? "••••••••" : item.value
            textFormat: Text.PlainText
            wrapMode: Text.WrapAnywhere
            font.family: item.secret || !item.monospace ? Theme.fontFamily : "monospace"
            color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
        }
    }

    IconButton {
        id: reveal
        objectName: "revealButton"
        anchors {
            right: parent.right
            rightMargin: Theme.horizontalPageMargin - Theme.paddingMedium
            verticalCenter: parent.verticalCenter
        }
        visible: item.revealable
        icon.source: item.secret ? "image://theme/icon-splus-show-password" : "image://theme/icon-splus-hide-password"
        onClicked: item.revealToggled()
    }
}
