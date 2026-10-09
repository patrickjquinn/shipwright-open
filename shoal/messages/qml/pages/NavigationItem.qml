// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// A row that opens a page, as Jolla Settings lists its pages: the name in
// the primary colour (highlighted while pressed) and, optionally, one line
// on what is there. Used instead of a column of buttons, which on Sailfish
// read as actions, not as places to go.

import QtQuick 2.0
import Sailfish.Silica 1.0

BackgroundItem {
    id: item

    property string text
    property string description

    width: parent ? parent.width : implicitWidth
    height: Math.max(Theme.itemSizeSmall, column.height + 2 * Theme.paddingMedium)

    Column {
        id: column
        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        anchors.verticalCenter: parent.verticalCenter

        Label {
            width: parent.width
            truncationMode: TruncationMode.Fade
            color: item.highlighted ? Theme.highlightColor : Theme.primaryColor
            text: item.text
        }
        Label {
            width: parent.width
            visible: text.length > 0
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: item.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            text: item.description
        }
    }
}
