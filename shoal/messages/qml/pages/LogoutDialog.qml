// Modified by Shipwright, 2026: Sailfish UI rules pass; see CHANGES-FROM-UPSTREAM.md.
import QtQuick 2.0
import Sailfish.Silica 1.0

// Signing out ends the session and clears the crypto store, so anything not in
// a key backup becomes unreadable. Worth a deliberate confirmation.
Dialog {
    id: dialog

    allowedOrientations: Orientation.All

    onAccepted: matrix.logout()

    Column {
        width: parent.width
        spacing: Theme.paddingLarge

        DialogHeader {
            acceptText: qsTr("Sign out")
            cancelText: qsTr("Stay signed in")
        }

        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.highlightColor
            text: qsTr("Really sign out?")
        }

        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryHighlightColor
            text: qsTr("This device's keys are deleted along with the session. Encrypted messages stay readable only if they are in a key backup, and this device has to be verified again after signing in.")
        }

        // Every sign-out has to say how to get back in where that is not obvious: on
        // Sailfish 4 the browser cannot finish an OAuth sign-in.
        Label {
            x: Theme.horizontalPageMargin
            width: parent.width - 2 * Theme.horizontalPageMargin
            wrapMode: Text.Wrap
            font.pixelSize: Theme.fontSizeSmall
            color: Theme.errorColor
            visible: !matrix.browserLoginReliable
            text: qsTr("On this Sailfish version the browser can't finish signing in. To sign back in, use “Sign in on another device”.")
        }
    }
}
