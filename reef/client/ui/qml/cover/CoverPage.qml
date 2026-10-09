// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Cover: the number of updates, or an install's progress, and a refresh
// action. Rendered through keel-shell's cover window (docs/plan.md,
// "keel-shell"; Phase 0 probe 3).

import QtQuick
import Sailfish.Silica 1.0
import Shipwright.Reef 1.0
import "../components/Texts.js" as Texts

CoverBackground {
    id: cover

    readonly property bool hasProgress: Reef.busy && Reef.progress >= 0

    Column {
        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            topMargin: Theme.paddingLarge
            leftMargin: Theme.paddingLarge
            rightMargin: Theme.paddingLarge
        }
        spacing: Theme.paddingSmall

        Item {
            anchors.horizontalCenter: parent.horizontalCenter
            width: Theme.iconSizeExtraLarge
            height: Theme.iconSizeExtraLarge

            // Updates waiting: their number, large.
            Label {
                objectName: "coverCount"
                anchors.centerIn: parent
                visible: !Reef.busy && Reef.updateCount > 0
                text: Reef.updateCount
                font.pixelSize: Theme.fontSizeHuge
            }
            // An operation with progress: a circle with the percentage.
            ProgressCircle {
                anchors.fill: parent
                visible: cover.hasProgress
                progressColor: Theme.highlightColor
                backgroundColor: Theme.highlightDimmerColor
                value: Math.max(0, Reef.progress) / 100
            }
            Label {
                anchors.centerIn: parent
                visible: cover.hasProgress
                text: qsTr("%1%").arg(Reef.progress)
            }
            BusyIndicator {
                anchors.centerIn: parent
                size: BusyIndicatorSize.Medium
                running: Reef.busy && !cover.hasProgress
                visible: running
            }
            Icon {
                anchors.centerIn: parent
                visible: !Reef.busy && Reef.updateCount === 0
                source: "image://theme/icon-m-cloud-download"
            }
        }
        Label {
            objectName: "coverState"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            text: Reef.busy ? Texts.busyText(Reef.busyText)
                : Reef.updateCount > 0 ? qsTr("update(s)", "", Reef.updateCount)
                : qsTr("Up to date")
        }
        Label {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryColor
            text: qsTr("Reef")
        }
    }

    CoverActionList {
        objectName: "coverActions"
        enabled: !Reef.busy
        CoverAction {
            iconSource: "image://theme/icon-cover-refresh"
            onTriggered: Reef.refresh()
        }
    }
}
