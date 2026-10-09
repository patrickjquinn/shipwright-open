// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// One tile of the Featured row on the catalogue page: the icon with the
// title under it. Same roles as PackageDelegate (featuredModel).

import QtQuick
import Sailfish.Silica 1.0

ListItem {
    id: tile

    required property string name
    required property string title
    required property string iconPath

    width: Theme.iconSizeLauncher + 2 * Theme.paddingLarge
    contentHeight: Theme.iconSizeLauncher + Theme.paddingSmall + titleLabel.height + 2 * Theme.paddingMedium

    AppIcon {
        id: appIcon
        anchors.top: parent.top
        anchors.topMargin: Theme.paddingMedium
        anchors.horizontalCenter: parent.horizontalCenter
        iconPath: tile.iconPath
        title: tile.title
    }

    Label {
        id: titleLabel
        anchors.top: appIcon.bottom
        anchors.topMargin: Theme.paddingSmall
        width: parent.width - 2 * Theme.paddingSmall
        anchors.horizontalCenter: parent.horizontalCenter
        horizontalAlignment: Text.AlignHCenter
        text: tile.title
        textFormat: Text.PlainText
        color: tile.highlighted ? Theme.highlightColor : Theme.primaryColor
        font.pixelSize: Theme.fontSizeExtraSmall
        truncationMode: TruncationMode.Fade
    }
}
