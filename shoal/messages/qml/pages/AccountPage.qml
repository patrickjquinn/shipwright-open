// Modified by Shipwright, 2026: Shipwright subscription and bridges entry points; left-aligned account details; the app's pages as navigation rows, About here; Sailfish UI rules pass; see CHANGES-FROM-UPSTREAM.md.
import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.Pickers 1.0

// The signed-in account: identity, own profile (display name and avatar) and
// the entry points for encryption and signing out.
Page {
    id: page

    allowedOrientations: Orientation.All

    Component.onCompleted: matrix.fetchProfile()

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column

            width: page.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Signed in")
            }

            DetailItem {
                // Left aligned: a long Matrix ID goes below the label, at
                // full width, instead of wrapping in half the page.
                alignment: Qt.AlignLeft
                label: qsTr("Account")
                value: matrix.userId
            }

            DetailItem {
                alignment: Qt.AlignLeft
                label: qsTr("Device")
                value: matrix.deviceId
            }

            DetailItem {
                alignment: Qt.AlignLeft
                label: qsTr("Core")
                value: matrix.coreVersion
            }

            // One direction, and here the other one: a wrapping button takes its width as
            // given, so deriving it from the children collapses every button on the page.
            readonly property real buttonWidth: Math.min(
                    Theme.buttonWidthLarge,
                    width - 2 * Theme.horizontalPageMargin)

            SectionHeader {
                text: qsTr("Profile")
            }

            TextField {
                id: nameField

                width: parent.width
                label: qsTr("Display name")
                placeholderText: qsTr("Display name")
                text: matrix.profileName
                EnterKey.iconSource: "image://theme/icon-m-accept"
                EnterKey.onClicked: {
                    matrix.setDisplayName(text)
                    focus = false
                }
            }

            WrapButton {
                id: saveNameButton

                anchors.horizontalCenter: parent.horizontalCenter
                width: column.buttonWidth
                label: qsTr("Save name")
                enabled: nameField.text !== matrix.profileName
                onClicked: {
                    matrix.setDisplayName(nameField.text)
                    nameField.focus = false
                }
            }

            WrapButton {
                id: avatarButton

                anchors.horizontalCenter: parent.horizontalCenter
                width: column.buttonWidth
                label: matrix.profileAvatar.length > 0
                      ? qsTr("Change avatar")
                      : qsTr("Set avatar")
                onClicked: pageStack.push(avatarPicker)
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                visible: matrix.profileAvatar.length > 0
                text: qsTr("An avatar is set. Other people see it next to your name.")
            }

            SectionHeader {
                text: qsTr("This app")
            }

            NavigationItem {
                id: appearanceButton
                text: qsTr("Appearance")
                description: qsTr("Bubbles, colours and the room list")
                onClicked: pageStack.push(Qt.resolvedUrl("AppearancePage.qml"))
            }

            NavigationItem {
                id: privacyButton
                text: qsTr("Privacy")
                description: qsTr("Who may call you, contacts")
                onClicked: pageStack.push(Qt.resolvedUrl("PrivacyPage.qml"))
            }

            NavigationItem {
                id: pushButton
                text: qsTr("Push notifications")
                onClicked: pageStack.push(Qt.resolvedUrl("PushPage.qml"))
            }

            // Shipwright: entry point for the subscription's onboarding bundle.
            NavigationItem {
                id: subscriptionButton
                text: qsTr("Shipwright subscription")
                onClicked: pageStack.push(Qt.resolvedUrl("SubscriptionPage.qml"))
            }

            // Shipwright: the hosted Signal and Telegram bridges.
            NavigationItem {
                id: bridgesButton
                text: qsTr("Signal and Telegram")
                onClicked: pageStack.push(Qt.resolvedUrl("BridgesPage.qml"))
            }

            NavigationItem {
                id: ignoredButton
                text: qsTr("Ignored users")
                onClicked: pageStack.push(Qt.resolvedUrl("IgnoredUsersPage.qml"))
            }

            ValueButton {
                label: qsTr("Language")
                value: {
                    var list = language.available
                    for (var i = 0; i < list.length; ++i) {
                        if (list[i].code === language.code) {
                            return list[i].name
                        }
                    }
                    return ""
                }
                onClicked: pageStack.push(Qt.resolvedUrl("LanguagePage.qml"))
            }

            // Shipwright: About moved here from the room list's pull-down
            // menu, which holds at most four entries.
            NavigationItem {
                text: qsTr("About Shoal Messages")
                onClicked: pageStack.push(Qt.resolvedUrl("AboutPage.qml"))
            }

            Item {
                width: 1
                height: Theme.paddingMedium
            }

            // The reset the send warning's subtitle promises. One button rather than a
            // list: the entries are addresses, and a list is a second place they stand.
            WrapButton {
                id: resetWarningsButton

                anchors.horizontalCenter: parent.horizontalCenter
                width: column.buttonWidth
                label: qsTr("Reset send warnings")
                onClicked: {
                    var count = matrix.resetRecipientWarnings()
                    resetWarningsHint.text = count > 0
                            ? qsTr("%n recipient(s) will warn again", "", count)
                            : qsTr("No suppressed warnings")
                    resetWarningsHint.opacity = 1.0
                    resetWarningsTimer.restart()
                }
            }

            Label {
                id: resetWarningsHint

                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                horizontalAlignment: Text.AlignHCenter
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                opacity: 0.0
                visible: opacity > 0
                Behavior on opacity { FadeAnimation { } }

                Timer {
                    id: resetWarningsTimer
                    interval: 3000
                    onTriggered: resetWarningsHint.opacity = 0.0
                }
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            // Said where somebody comes past it, not only where somebody goes looking.
            // Only where a key could be had - a line that offers nothing is a nag.
            BackgroundItem {
                width: parent.width
                height: unencryptedWarning.height + 2 * Theme.paddingMedium
                visible: !matrix.storageStatus.encrypted
                         && matrix.storageStatus.keyAvailable === true
                onClicked: pageStack.push(Qt.resolvedUrl("EncryptionPage.qml"))

                Label {
                    id: unencryptedWarning

                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.errorColor
                    text: qsTr("Session and message database lie on this device unencrypted. Tap to encrypt them.")
                }
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.errorColor
                visible: matrix.lastError.length > 0
                text: matrix.lastError
            }
        }

        PullDownMenu {
            // Topmost, which is what a full pull reaches: signing out is looked for once,
            // after something went wrong, and never in passing.
            MenuItem {
                text: qsTr("Error log")
                onClicked: pageStack.push(Qt.resolvedUrl("ErrorLogPage.qml"))
            }

            MenuItem {
                text: qsTr("Sign out")
                onClicked: pageStack.push(Qt.resolvedUrl("LogoutDialog.qml"))
            }

            MenuItem {
                text: qsTr("Encryption")
                onClicked: pageStack.push(Qt.resolvedUrl("EncryptionPage.qml"))
            }
        }

        VerticalScrollDecorator { }
    }

    Component {
        id: avatarPicker

        // Same as the attachment picker: the platform pushes its own sub-pages
        // without passing an orientation on, so this one must not declare one.
        ImagePickerPage {
            onSelectedContentPropertiesChanged: {
                if (selectedContentProperties.filePath.length > 0) {
                    matrix.setAvatarFile(selectedContentProperties.filePath)
                }
            }
        }
    }
}
