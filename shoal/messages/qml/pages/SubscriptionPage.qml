// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
// Minimal entry point for a Shipwright subscription: apply the onboarding
// bundle printed by services/deploy/provision-subscriber.sh, then go on to
// link Signal and Telegram (BridgesPage.qml). The proper onboarding flow
// (sign-up, sign-in) is in ROADMAP.md.
import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0

Page {
    id: page

    allowedOrientations: Orientation.All

    property bool applying: false
    property string message: ""
    property bool failed: false
    property bool applied: false

    Connections {
        target: matrix
        onHostedPushApplied: {
            page.applying = false
            page.failed = false
            page.applied = true
            page.message = moved > 0
                    ? qsTr("Shoal Push is set up for your subscription. %n app registration(s) moved to it.", "", moved)
                    : qsTr("Shoal Push is set up for your subscription.")
            if (enablePush.checked && !settings.pushEnabled) {
                settings.pushEnabled = true
                matrix.enablePush(settings.pushGateway, settings.pushDistributor)
            }
        }
        onHostedPushFailed: {
            page.applying = false
            page.failed = true
            page.message = reason
        }
    }

    function apply(text) {
        page.applying = true
        page.message = ""
        matrix.applySubscriptionBundle(text)
        // The bundle holds the push token and possibly a password: not kept on screen.
        bundleField.text = ""
    }

    Component {
        id: bundlePicker
        FilePickerPage {
            nameFilters: ["*.json", "*.txt"]
            onSelectedContentPropertiesChanged: {
                var url = selectedContentProperties.url || ("file://" + selectedContentProperties.filePath)
                var request = new XMLHttpRequest()
                request.onreadystatechange = function() {
                    if (request.readyState === XMLHttpRequest.DONE) {
                        if (request.responseText.length > 0) {
                            page.apply(request.responseText)
                        } else {
                            page.failed = true
                            page.message = qsTr("The file could not be read.")
                        }
                    }
                }
                request.open("GET", url)
                request.send()
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        VerticalScrollDecorator {}

        Column {
            id: column

            width: page.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Shipwright subscription")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Paste the onboarding bundle you received with your subscription, or open it from a file. It sets up Shoal Push with your own push account and the Shoal push gateway. The bundle contains secrets: it is not stored by this app.")
            }

            TextArea {
                id: bundleField

                width: parent.width
                label: qsTr("Onboarding bundle")
                placeholderText: qsTr("Onboarding bundle (JSON)")
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText | Qt.ImhSensitiveData
                enabled: !page.applying
            }

            TextSwitch {
                id: enablePush

                text: qsTr("Switch on push notifications")
                description: qsTr("Uses Shoal Push unless you picked another distributor.")
                checked: true
                visible: !settings.pushEnabled
            }

            WrapButton {
                anchors.horizontalCenter: parent.horizontalCenter
                label: qsTr("Apply")
                enabled: !page.applying && bundleField.text.trim().length > 0
                onClicked: page.apply(bundleField.text)
            }

            WrapButton {
                anchors.horizontalCenter: parent.horizontalCenter
                label: qsTr("Paste from clipboard")
                enabled: !page.applying && Clipboard.hasText
                onClicked: {
                    page.apply(Clipboard.text)
                    Clipboard.text = ""
                }
            }

            WrapButton {
                anchors.horizontalCenter: parent.horizontalCenter
                label: qsTr("Open bundle file")
                enabled: !page.applying
                onClicked: pageStack.push(bundlePicker)
            }

            BusyIndicator {
                anchors.horizontalCenter: parent.horizontalCenter
                size: BusyIndicatorSize.Medium
                running: page.applying
                visible: running
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: page.message.length > 0
                font.pixelSize: Theme.fontSizeSmall
                color: page.failed ? Theme.errorColor : Theme.highlightColor
                text: page.message
            }

            SectionHeader {
                text: qsTr("Signal and Telegram")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Your subscription includes hosted bridges to Signal and Telegram. Link your accounts to read and answer those chats here.")
            }

            WrapButton {
                anchors.horizontalCenter: parent.horizontalCenter
                label: page.applied ? qsTr("Next: link Signal and Telegram") : qsTr("Link Signal and Telegram")
                enabled: !page.applying && matrix.sessionState === "signed-in"
                onClicked: pageStack.push(Qt.resolvedUrl("BridgesPage.qml"))
            }
        }
    }
}
