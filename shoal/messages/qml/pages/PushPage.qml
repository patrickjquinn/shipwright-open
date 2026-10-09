// Modified by Shipwright, 2026: rebranded as Shoal Messages; Sailfish UI rules pass; see CHANGES-FROM-UPSTREAM.md.
import QtQuick 2.0
import Sailfish.Silica 1.0

import "SecurityStatus.js" as SecurityStatus

// Push through a UnifiedPush distributor - the daemon is the distributor, this
// is where the arrangement is turned on and says what it is doing.
Page {
    id: page

    allowedOrientations: Orientation.All

    readonly property var pushStatus: matrix.pushStatus
    readonly property string pushState: pushStatus.state || ""
    readonly property var distributors: pushStatus.distributors || []
    // Shipwright: the distributor in use, else the user's pick, else what
    // "Automatic" would take (Shoal Push when it is on the bus).
    readonly property string effectiveDistributor: pushStatus.distributor
                                                   || settings.pushDistributor
                                                   || pushStatus.automatic || ""

    // `org.unifiedpush.Distributor.<name>`: only the tail names the app.
    function shortName(busName) {
        var tail = String(busName).split(".").pop()
        return busName === pushStatus.shoalDistributor ? qsTr("Shoal Push") : tail
    }

    Component.onCompleted: matrix.refreshPushStatus()

    // On every visit, not once: a distributor can be installed or removed while
    // this app runs, and a cached answer would be wrong exactly then.
    onStatusChanged: {
        if (status === PageStatus.Active) {
            matrix.refreshPushStatus()
        }
    }

    function apply(on) {
        settings.pushEnabled = on
        if (on) {
            matrix.enablePush(settings.pushGateway, settings.pushDistributor)
        } else {
            matrix.disablePush()
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
                title: qsTr("Push notifications")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Shoal Messages has no background service, so messages arrive only while it runs. A push distributor is a separate app that holds one connection for every app on the device and wakes them when something comes in.")
            }

            TextSwitch {
                text: qsTr("Receive push notifications")
                checked: settings.pushEnabled
                automaticCheck: false
                // Switching *on* needs both halves - the line below promises that.
                // Switching off must always be possible: a distributor that is
                // uninstalled or merely not running right now would otherwise
                // leave this on, unreachable, with the pusher still registered on
                // the homeserver and the metadata still flowing.
                enabled: settings.pushEnabled
                         || (settings.pushGateway.trim().length > 0
                             && page.distributors.length > 0)
                onClicked: page.apply(!settings.pushEnabled)
            }

            SecurityRow {
                label: qsTr("Distributor")
                level: page.distributors.length > 0 ? SecurityStatus.GREEN
                                                    : SecurityStatus.RED
                detail: page.distributors.length > 0
                        ? page.shortName(page.effectiveDistributor)
                        : qsTr("No push distributor is installed. Install Shoal Push, or another UnifiedPush distributor; without one there is nothing to hold the connection, and this stays off.")
            }

            // Shipwright: the user may pick any distributor on the device.
            // "Automatic" prefers Shoal Push (see core/src/push.rs).
            ComboBox {
                id: distributorBox

                width: parent.width
                visible: page.distributors.length > 0
                label: qsTr("Use distributor")
                // Changing it means registering again; off first keeps the
                // pusher on the homeserver and the registration in step.
                enabled: !settings.pushEnabled
                description: settings.pushEnabled
                             ? qsTr("Switch push notifications off to choose another distributor.")
                             : qsTr("Automatic uses Shoal Push when it is installed.")
                currentIndex: {
                    var index = page.distributors.indexOf(settings.pushDistributor)
                    return settings.pushDistributor.length > 0 && index >= 0 ? index + 1 : 0
                }
                menu: ContextMenu {
                    MenuItem {
                        text: qsTr("Automatic")
                        onClicked: settings.pushDistributor = ""
                    }
                    Repeater {
                        model: page.distributors
                        MenuItem {
                            text: page.shortName(modelData)
                            onClicked: settings.pushDistributor = modelData
                        }
                    }
                }
            }

            SecurityRow {
                label: qsTr("Registration")
                level: matrix.pushEndpointReady
                       ? SecurityStatus.GREEN
                       : (page.pushState === "registering" ? SecurityStatus.ORANGE
                                                       : SecurityStatus.RED)
                detail: {
                    if (matrix.pushEndpointReady) {
                        return qsTr("This device has an address to be reached at.")
                    }
                    if (page.pushState === "registering") {
                        return qsTr("Waiting for the distributor.")
                    }
                    return qsTr("Not registered.")
                }
            }

            SectionHeader {
                text: qsTr("Gateway")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("A Matrix homeserver cannot talk to a push distributor directly, so it posts to a gateway that forwards. The Shoal gateway is used unless you enter your own; clear the field to go back to it.")
            }

            TextField {
                id: gatewayField

                width: parent.width
                text: settings.pushGateway
                label: qsTr("Push gateway")
                placeholderText: settings.defaultPushGateway
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                EnterKey.iconSource: "image://theme/icon-m-enter-accept"
                EnterKey.onClicked: {
                    settings.pushGateway = text
                    // Cleared means the built-in gateway again; show it.
                    text = settings.pushGateway
                    focus = false
                }
            }

            SectionHeader {
                text: qsTr("What leaves this device")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeExtraSmall
                color: Theme.secondaryHighlightColor
                text: qsTr("Your server sends each notification to an address at the push service. It contains only room and message IDs, never text: this phone fetches and decrypts the message itself. Keep the address secret, because anyone with it can send this phone notifications.")
            }

            Label {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: (page.pushStatus.error || "").length > 0
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.errorColor
                text: page.pushStatus.error || ""
            }
        }
    }
}
