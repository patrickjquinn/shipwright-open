// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// One package row of catalogueModel or installedModel: icon, title, who
// publishes it, summary and install state. The roles arrive as required
// properties (README, "catalogueModel"). Title, publisher and summary come
// from the developer's catalogue entry, so they are plain text.

import QtQuick
import Sailfish.Silica 1.0

ListItem {
    id: row

    required property string name
    required property string title
    required property string summary
    required property string installState
    required property string installedVersion
    required property string version
    required property string licenceModel
    required property bool licensed
    required property string publisher
    required property string iconPath
    required property bool isNew

    contentHeight: Math.max(Theme.itemSizeLarge, column.height + 2 * Theme.paddingMedium)

    function stateText() {
        // Versions are on the app's page; the list says what matters.
        if (installState === "update_available")
            return qsTr("Update")
        if (installState === "installed")
            return qsTr("Installed")
        if (licenceModel === "one_off")
            return licensed ? qsTr("Licensed") : qsTr("Paid")
        if (licenceModel === "subscription")
            return licensed ? qsTr("Subscribed") : qsTr("Subscription")
        return qsTr("Free")
    }

    AppIcon {
        id: appIcon
        anchors {
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }
        iconPath: row.iconPath
        title: row.title
    }

    Column {
        id: column
        anchors {
            left: appIcon.right
            right: parent.right
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.horizontalPageMargin
            verticalCenter: parent.verticalCenter
        }

        Item {
            width: parent.width
            height: titleLabel.height

            Label {
                id: titleLabel
                anchors.left: parent.left
                anchors.right: newLabel.visible ? newLabel.left : stateLabel.left
                anchors.rightMargin: Theme.paddingMedium
                text: row.title
                textFormat: Text.PlainText
                color: row.highlighted ? Theme.highlightColor : Theme.primaryColor
                truncationMode: TruncationMode.Fade
            }

            Label {
                id: newLabel
                anchors.right: stateLabel.left
                anchors.rightMargin: Theme.paddingMedium
                anchors.baseline: titleLabel.baseline
                visible: row.isNew && row.installState === "not_installed"
                text: qsTr("New")
                color: Theme.highlightColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }

            Label {
                id: stateLabel
                anchors.right: parent.right
                anchors.baseline: titleLabel.baseline
                text: row.stateText()
                textFormat: Text.PlainText
                color: row.installState === "update_available" ? Theme.highlightColor
                                                               : Theme.secondaryColor
                font.pixelSize: Theme.fontSizeExtraSmall
            }
        }

        Label {
            width: parent.width
            visible: row.publisher.length > 0
            text: row.publisher
            textFormat: Text.PlainText
            color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            truncationMode: TruncationMode.Fade
        }

        Label {
            width: parent.width
            text: row.summary
            textFormat: Text.PlainText
            color: row.highlighted ? Theme.secondaryHighlightColor : Theme.secondaryColor
            font.pixelSize: Theme.fontSizeExtraSmall
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
    }
}
