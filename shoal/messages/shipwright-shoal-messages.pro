# Modified by Shipwright, 2026: rebranded as Shoal Messages; bridges and contacts sources, QtContacts, bridge first-run and contact-picker pages, Keel Actions; see CHANGES-FROM-UPSTREAM.md.
# Shoal Messages — Sailfish OS qmake project.
#
# The Matrix protocol core is a Rust static library that is cross-built OUTSIDE
# this build, on the host, by scripts/build-core.sh. qmake only links it. Run
# that script once before the first build and again after touching core/.

TARGET = shipwright-shoal-messages

CONFIG += sailfishapp sailfishapp_i18n c++11

QT += network dbus multimedia concurrent positioning
# Shipwright: the address book, read-only (src/contactsbridge.cpp).
QT += contacts

# The media half of a call: Matrix carries only the signalling.
#
# pkg-config is queried directly rather than through link_pkgconfig: enabling
# that feature here resolves PKGCONFIG before sailfishapp.prf has added its own
# entry, which silently drops -lsailfishapp from the link line.
GST_MODULES = gstreamer-1.0 gstreamer-sdp-1.0 gstreamer-webrtc-1.0 gstreamer-video-1.0
QMAKE_CXXFLAGS += $$system(pkg-config --cflags $$GST_MODULES)
QMAKE_CXXFLAGS += $$system(pkg-config --cflags libpulse)

# The store key lives in Sailfish Secrets; same direct pkg-config route as
# GStreamer, for the same sailfishapp.prf reason.
QMAKE_CXXFLAGS += $$system(pkg-config --cflags sailfishsecrets)

# Keep the build machine out of the binary: __FILE__ and Qt's assertions would
# otherwise embed the absolute source path of whoever compiled it.
QMAKE_CXXFLAGS += -ffile-prefix-map=$$PWD=/build

# Binary hardening. FORTIFY needs optimisation, which the release build has.
# stack-protector-strong guards return addresses; full RELRO plus BIND_NOW
# makes the GOT read-only after load; the binary is already position
# independent. These raise the cost of exploiting a decoder bug in a received
# image or video, which is the app's main untrusted-input surface.
QMAKE_CXXFLAGS += -D_FORTIFY_SOURCE=2 -fstack-protector-strong
QMAKE_LFLAGS += -Wl,-z,relro -Wl,-z,now
LIBS += $$system(pkg-config --libs $$GST_MODULES)
LIBS += $$system(pkg-config --libs libpulse)
LIBS += $$system(pkg-config --libs sailfishsecrets)

SOURCES += \
    src/shipwright-shoal-messages.cpp \
    src/appearancesettings.cpp \
    src/appservice.cpp \
    src/appsettings.cpp \
    src/instancelock.cpp \
    src/pushwake.cpp \
    src/languagesettings.cpp \
    src/difflistmodel.cpp \
    src/emojiset.cpp \
    src/emojistore.cpp \
    src/emojiimageprovider.cpp \
    src/outgoingimage.cpp \
    src/videostill.cpp \
    src/imagefacts.cpp \
    src/matrixbridge.cpp \
    src/pollactions.cpp \
    src/bridgeactions.cpp \
    src/contactsbridge.cpp \
    src/keelactions.cpp \
    src/sendqueueactions.cpp \
    src/locationactions.cpp \
    src/camerashots.cpp \
    src/readingpositions.cpp \
    src/roomsettings.cpp \
    src/spacemarkers.cpp \
    src/linkpreviews.cpp \
    src/mentions.cpp \
    src/voicedecode.cpp \
    src/voicetranscripts.cpp \
    src/roomlistmodel.cpp \
    src/roomsortmodel.cpp \
    src/directorymodel.cpp \
    src/membermodel.cpp \
    src/searchmodel.cpp \
    src/timelinemodel.cpp \
    src/callengine.cpp \
    src/camerasource.cpp \
    src/callaudiorouter.cpp \
    src/videostream.cpp \
    src/voicerecorder.cpp \
    src/secretskeeper.cpp

HEADERS += \
    src/appearancesettings.h \
    src/appservice.h \
    src/appsettings.h \
    src/difflistmodel.h \
    src/emojiset.h \
    src/emojistore.h \
    src/emojiimageprovider.h \
    src/instancelock.h \
    src/pushwake.h \
    src/languagesettings.h \
    src/matrixbridge.h \
    src/pollactions.h \
    src/bridgeactions.h \
    src/contactsbridge.h \
    src/keelactions.h \
    src/sendqueueactions.h \
    src/locationactions.h \
    src/camerashots.h \
    src/readingpositions.h \
    src/roomsettings.h \
    src/spacemarkers.h \
    src/linkpreviews.h \
    src/mentions.h \
    src/voicedecode.h \
    src/voicetranscripts.h \
    src/outgoingimage.h \
    src/videostill.h \
    src/imagefacts.h \
    src/secretskeeper.h \
    src/shoalconfig.h \
    src/roomlistmodel.h \
    src/roomsortmodel.h \
    src/directorymodel.h \
    src/membermodel.h \
    src/searchmodel.h \
    src/timelinemodel.h \
    src/callengine.h \
    src/camerasource.h \
    src/callaudiorouter.h \
    src/videostream.h \
    src/voicerecorder.h

# cbindgen writes the C ABI header here (generated, not tracked).
INCLUDEPATH += src/generated

# The package version reaches the build as DEFINES+=SHOAL_MESSAGES_VERSION=... from
# the spec. Baked in as a bare command-line define it survives inside a stale
# object file — make only compares file times, so a rebuild where only the
# version changed shipped an About page naming the previous release. Routed
# through a generated header instead, every qmake run touches a file the
# compiler depends on, and the string can never outlive the build that set it.
SHOAL_MESSAGES_VERSION_VALUE = dev
for(def, DEFINES) {
    contains(def, "SHOAL_MESSAGES_VERSION=.*") {
        SHOAL_MESSAGES_VERSION_VALUE = $$replace(def, "SHOAL_MESSAGES_VERSION=", "")
    }
}
VERSION_HEADER_LINES = \
    "// Generated by qmake — do not edit." \
    "$${LITERAL_HASH}define SHOAL_MESSAGES_VERSION_STRING \"$$SHOAL_MESSAGES_VERSION_VALUE\""
!write_file($$PWD/src/generated/shoal_messages_version.h, VERSION_HEADER_LINES) {
    error("could not write src/generated/shoal_messages_version.h")
}

# --- Rust core --------------------------------------------------------------
# One Rust triple per RPM architecture; QT_ARCH names the target the qmake
# in the build chroot was built for.
equals(QT_ARCH, arm64): RUST_TARGET = aarch64-unknown-linux-gnu
else:equals(QT_ARCH, arm): RUST_TARGET = armv7-unknown-linux-gnueabihf
else:equals(QT_ARCH, i386): RUST_TARGET = i586-unknown-linux-gnu
else: error("Unsupported QT_ARCH: $$QT_ARCH")
CORE_LIB = $$PWD/core/target/$$RUST_TARGET/release/libshoal_messages_core.a

!exists($$CORE_LIB) {
    error("Rust core not built: $$CORE_LIB is missing. Run scripts/build-core.sh first.")
}

# The core reports its version via xm_version(), and a stale library once
# shipped trailing the package by two releases. build-core.sh stamps the
# version it built next to the archive; anything else is refused here, so
# bumping the spec without rebuilding the core stops the build instead of
# silently linking old code.
CORE_STAMP = $${CORE_LIB}.version
!exists($$CORE_STAMP) {
    error("No version stamp next to the Rust core ($$CORE_STAMP). Re-run scripts/build-core.sh.")
}
CORE_VERSION = $$cat($$CORE_STAMP)
SPEC_VERSION = $$system(grep ^Version: $$PWD/rpm/shipwright-shoal-messages.spec)
SPEC_VERSION = $$last(SPEC_VERSION)
!equals(CORE_VERSION, $$SPEC_VERSION) {
    error("Rust core is $${CORE_VERSION} but rpm/shipwright-shoal-messages.spec says $${SPEC_VERSION}. Re-run scripts/build-core.sh.")
}

LIBS += $$CORE_LIB -lpthread -ldl -lm -lrt
PRE_TARGETDEPS += $$CORE_LIB

# Installed to /usr/share/icons/hicolor/<size>/apps/ (see sailfishapp.prf).
SAILFISHAPP_ICONS = 86x86 108x108 128x128 172x172

# Lets the share dialog start the app when it is not already running. The file
# name has to be the D-Bus name, which Sailjail fixes; see the file itself.
dbusservice.files = org.shipwright.ShoalMessages.service
dbusservice.path = /usr/share/dbus-1/services
INSTALLS += dbusservice

# The second activation entry: a push arriving while the app is closed. Same
# directory, different name — see the file itself for why it cannot be a child
# of the app's own name.
pushservice.files = org.unifiedpush.Connector.ShoalMessages.service
pushservice.path = /usr/share/dbus-1/services
INSTALLS += pushservice

# Shipwright: Keel Actions (ADR-0018) for Pilot. core/build.rs generates the
# manifest and the D-Bus interface description from core/src/actions.rs and
# the room page's KeelContext; build-core.sh leaves them next to the core.
# keel-mcp reads the manifest from /usr/share/keel/actions. The generated
# activation file is not installed: Sailjail grants this app one bus name, and
# org.shipwright.ShoalMessages.service already activates it (for the share
# dialog); a Keel call that finds the app closed starts it through that file.
KEEL_ACTIONS = $$PWD/core/target/$$RUST_TARGET/release/keel-actions/org.shipwright.ShoalMessages
!exists($$KEEL_ACTIONS/actions.json) {
    error("No Keel Actions manifest next to the Rust core ($$KEEL_ACTIONS). Re-run scripts/build-core.sh.")
}
# Installed under the app ID, which the generated file is not named after.
keelmanifest.path = /usr/share/keel/actions
keelmanifest.extra = install -D -m 0644 $$KEEL_ACTIONS/actions.json $(INSTALL_ROOT)/usr/share/keel/actions/org.shipwright.ShoalMessages.json
INSTALLS += keelmanifest
keelinterface.files = $$KEEL_ACTIONS/org.shipwright.ShoalMessages.actions.xml
keelinterface.path = /usr/share/dbus-1/interfaces
INSTALLS += keelinterface

# The link handler. Its own file so the launcher grid keeps one Shoal Messages; the app
# itself is started against the main desktop file, see the file for why.
urlhandler.files = shipwright-shoal-messages-open-url.desktop
urlhandler.path = /usr/share/applications
INSTALLS += urlhandler

# The sandbox's leave to talk to the offline speech service, for a voice message
# converted to text on request. Sailjail reads permissions from this directory only.
speechpermission.files = ShoalMessagesSpeech.permission
speechpermission.path = /etc/sailjail/permissions
INSTALLS += speechpermission

# One second of silence in Ogg/Opus: the check runs the real path with it.
voicecheck.files = data/voice-check.opus
voicecheck.path = /usr/share/$${TARGET}/data
INSTALLS += voicecheck

# English is the source language; the files below carry the translations.
# libsailfishapp loads the .qm matching the device locale and falls back to the
# source strings where there is none.
#
# EU official languages plus Russian, Norwegian Bokmål and Icelandic. English
# has a file of its own although it is the source language: picking it in the
# app has to override the automatic translator, and only a real .qm can do
# that. A language is listed here only once actually translated; review state
# per language is in translations/STATUS.md.
TRANSLATIONS += \
    translations/shipwright-shoal-messages-ar.ts \
    translations/shipwright-shoal-messages-bg.ts \
    translations/shipwright-shoal-messages-cs.ts \
    translations/shipwright-shoal-messages-da.ts \
    translations/shipwright-shoal-messages-de.ts \
    translations/shipwright-shoal-messages-el.ts \
    translations/shipwright-shoal-messages-en.ts \
    translations/shipwright-shoal-messages-es.ts \
    translations/shipwright-shoal-messages-et.ts \
    translations/shipwright-shoal-messages-fa.ts \
    translations/shipwright-shoal-messages-fi.ts \
    translations/shipwright-shoal-messages-fr.ts \
    translations/shipwright-shoal-messages-ga.ts \
    translations/shipwright-shoal-messages-hi.ts \
    translations/shipwright-shoal-messages-hr.ts \
    translations/shipwright-shoal-messages-hu.ts \
    translations/shipwright-shoal-messages-is.ts \
    translations/shipwright-shoal-messages-it.ts \
    translations/shipwright-shoal-messages-ja.ts \
    translations/shipwright-shoal-messages-lt.ts \
    translations/shipwright-shoal-messages-lv.ts \
    translations/shipwright-shoal-messages-mt.ts \
    translations/shipwright-shoal-messages-nb.ts \
    translations/shipwright-shoal-messages-nl.ts \
    translations/shipwright-shoal-messages-pl.ts \
    translations/shipwright-shoal-messages-pt.ts \
    translations/shipwright-shoal-messages-ro.ts \
    translations/shipwright-shoal-messages-ru.ts \
    translations/shipwright-shoal-messages-sk.ts \
    translations/shipwright-shoal-messages-sl.ts \
    translations/shipwright-shoal-messages-sv.ts \
    translations/shipwright-shoal-messages-zh_CN.ts \

# The QML rule check runs with every build, not when somebody remembers it: a
# second handler for one signal makes a page unloadable, and neither qmake nor
# qmllint says a word about it.
qmlcheck.commands = python3 $$PWD/scripts/qml-check.py $$PWD/qml
qmlcheck.depends =
QMAKE_EXTRA_TARGETS += qmlcheck
PRE_TARGETDEPS += qmlcheck

# The link handler's sandbox profile must grant what the app's does. It said so
# in a comment and was one entry short for two releases; the comment is now a
# check that fails the build.
desktopcheck.commands = python3 $$PWD/scripts/desktop-check.py $$PWD
desktopcheck.depends =
QMAKE_EXTRA_TARGETS += desktopcheck
PRE_TARGETDEPS += desktopcheck

DISTFILES += \
    LICENSE \
    README.md \
    shipwright-shoal-messages.desktop \
    shipwright-shoal-messages-open-url.desktop \
    ShoalMessagesSpeech.permission \
    data/voice-check.opus \
    rpm/shipwright-shoal-messages.spec \
    icons/shoal-messages-logo.svg \
    qml/shipwright-shoal-messages.qml \
    qml/cover/CoverPage.qml \
    qml/pages/AccountPage.qml \
    qml/pages/LoginPage.qml \
    qml/pages/SendQueueBanner.qml \
    qml/pages/SessionLockedPage.qml \
    qml/pages/SessionNewerPage.qml \
    qml/pages/SessionOfflinePage.qml \
    qml/pages/AboutPage.qml \
    qml/pages/RoomListPage.qml \
    qml/pages/RoomDelegate.qml \
    qml/pages/SpacesPage.qml \
    qml/pages/SpacePage.qml \
    qml/pages/SpaceColourPage.qml \
    qml/pages/CreateSpaceDialog.qml \
    qml/pages/AddToSpacePage.qml \
    qml/pages/MoveToSpacePage.qml \
    qml/pages/RoomPage.qml \
    qml/pages/ConversationContext.qml \
    qml/pages/Composer.qml \
    qml/pages/PollBlock.qml \
    qml/pages/LocationBlock.qml \
    qml/pages/LocationMap.qml \
    qml/pages/CameraCapturePage.qml \
    qml/pages/ShareLocationPage.qml \
    qml/pages/LinkPreviewCard.qml \
    qml/pages/CreatePollDialog.qml \
    qml/pages/Composing.js \
    qml/pages/FormatBar.qml \
    qml/pages/AttachmentPickerPage.qml \
    qml/pages/ModeTab.qml \
    qml/pages/RoomInfoPage.qml \
    qml/pages/RoomSettingsPage.qml \
    qml/pages/SearchPage.qml \
    qml/pages/ShieldGlossaryPage.qml \
    qml/pages/AppearancePage.qml \
    qml/pages/EmojiFolderPage.qml \
    qml/pages/PrivacyPage.qml \
    qml/pages/PushPage.qml \
    qml/pages/SubscriptionPage.qml \
    qml/pages/BridgesPage.qml \
    qml/pages/BridgeLinkPage.qml \
    qml/pages/BridgesIntroDialog.qml \
    qml/pages/ContactPickerPage.qml \
    qml/pages/BridgedChatPage.qml \
    qml/pages/ColorField.qml \
    qml/pages/VerificationPage.qml \
    qml/pages/VerifyUserPage.qml \
    qml/pages/EncryptionPage.qml \
    qml/pages/ErrorLogPage.qml \
    qml/pages/LogoutDialog.qml \
    qml/pages/EncryptStorageDialog.qml \
    qml/pages/WrapButton.qml \
    qml/pages/NavigationItem.qml \
    qml/pages/SecurityStatus.js \
    qml/pages/SecurityStatusPage.qml \
    qml/pages/SecurityLamp.qml \
    qml/pages/SecurityRow.qml \
    qml/pages/SecurityRows.qml \
    qml/pages/StorageBlockedPage.qml \
    qml/pages/SecretsHelpPage.qml \
    qml/pages/LanguagePage.qml \
    qml/pages/Formatting.js \
    qml/pages/MatrixLinks.js \
    qml/pages/Preview.js \
    qml/pages/ConfirmDialog.qml \
    qml/pages/ImageViewPage.qml \
    qml/pages/ForwardPage.qml \
    qml/pages/ShareToRoomPage.qml \
    qml/pages/MessageActionsPage.qml \
    qml/pages/ReactionDialog.qml \
    qml/pages/EmojiItem.qml \
    qml/pages/Emoji.js \
    qml/pages/RoomActionsPage.qml \
    qml/pages/Avatar.qml \
    qml/pages/VideoPage.qml \
    qml/pages/JoinRoomDialog.qml \
    qml/pages/NewChatDialog.qml \
    qml/pages/CreateRoomDialog.qml \
    qml/pages/InviteToRoomDialog.qml \
    qml/pages/DirectoryPage.qml \
    qml/pages/AddDirectoryServerDialog.qml \
    qml/pages/PinnedMessagesPage.qml \
    qml/pages/MemberListPage.qml \
    qml/pages/MentionPicker.qml \
    qml/pages/MemberProfilePage.qml \
    qml/pages/IgnoredUsersPage.qml \
    qml/pages/ThreadPage.qml \
    qml/pages/CallPage.qml \
    qml/pages/SendMediaPage.qml

lupdate_only {
    SOURCES += qml/*.qml qml/cover/*.qml qml/pages/*.qml qml/pages/*.js
}
