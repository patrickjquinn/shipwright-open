// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// One tile of the home page's "New and updated" row: the icon, the title,
// and whether it is new or when it was last updated (or that an update is
// waiting). Same roles as PackageDelegate (recentModel).

import QtQuick
import Sailfish.Silica 1.0
import "Texts.js" as Texts

ListItem {
    id: tile

    required property string name
    required property string title
    required property string iconPath
    required property bool isNew
    required property string updated
    required property string installState

    width: Theme.iconSizeLauncher + 2 * Theme.paddingLarge + Theme.paddingMedium
    contentHeight: Theme.iconSizeLauncher + Theme.paddingSmall + titleLabel.height + captionLabel.height + 2 * Theme.paddingMedium

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
        // Centred while it fits; too long, it starts at the left and fades out.
        horizontalAlignment: implicitWidth > width ? Text.AlignLeft : Text.AlignHCenter
        text: tile.title
        color: tile.highlighted ? Theme.highlightColor : Theme.primaryColor
        font.pixelSize: Theme.fontSizeExtraSmall
        truncationMode: TruncationMode.Fade
    }

    Label {
        id: captionLabel
        anchors.top: titleLabel.bottom
        width: parent.width - 2 * Theme.paddingSmall
        anchors.horizontalCenter: parent.horizontalCenter
        // Centred while it fits; too long, it starts at the left and fades out.
        horizontalAlignment: implicitWidth > width ? Text.AlignLeft : Text.AlignHCenter
        text: tile.installState === "update_available" ? qsTr("Update")
            : tile.isNew ? qsTr("New")
            : Texts.date(tile.updated).replace(/\s*\d{4}$/, "")
        color: tile.installState === "update_available" || tile.isNew ? Theme.highlightColor : Theme.secondaryColor
        font.pixelSize: Theme.fontSizeTiny
        truncationMode: TruncationMode.Fade
    }
}
