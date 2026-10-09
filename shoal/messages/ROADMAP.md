<!-- SPDX-FileCopyrightText: 2026 Patrick Quinn -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Shoal Messages roadmap (Phase 2)

Phase 2 is the "Killer app" block of `docs/plan.md`, 4 January to 31 March 2027,
ending at Gate 2: messages and push reliable on a stock phone, with a working
subscription lifecycle. Paths are relative to `shoal/messages/`. Every new
command follows the existing pattern: a variant in `core/src/protocol.rs`, a
handler in `core/src/runtime.rs`, a `Q_INVOKABLE` plus event handling in
`src/matrixbridge.{h,cpp}`, and QML on top.

## 1. Hosted Signal and Telegram bridges: onboarding UI

**Status 2026-10-01:** linking, status and unlinking done off-device
(`core/src/bridges.rs`, `BridgesPage.qml`, `BridgeLinkPage.qml`; README,
"Hosted bridges"). Second pass the same day: bridged rooms are marked in the
chat list and room info (`bridge` field from heroes, direct targets and
`io.element.functional_members`), Privacy has the bridged-chats copy, and the
bridges page is offered once after sign-in (`BridgesIntroDialog.qml`,
`bridges/introDone`). All of it needs device verification against a real cell.
Still open: bridge credentials and retention wording matched to the published
privacy policy, which does not exist yet.

The bridges run on our homeserver (`services/`). The client has to link and
unlink an external account, and label bridged rooms honestly.

- **Talking to the bridge bot.** mautrix bridges (Signal, Telegram) are driven
  by commands in a DM with the bridge bot (`login`, `logout`, QR or phone
  code). New `core/src/bridges.rs`: open or find the bot DM (reuse DM creation
  from `core/src/runtime.rs`, the code behind `NewChatDialog.qml`), send
  commands, and parse the bot's replies (QR payload, "enter code", success,
  failure) into structured events `bridge.state` and `bridge.qr`.
- **Pages.** New `qml/pages/BridgesPage.qml` (list: Signal, Telegram, state,
  link and unlink) and `qml/pages/BridgeLinkPage.qml` (QR rendered locally;
  phone number and code entry for Telegram). Entry from
  `qml/pages/AccountPage.qml`, and from first-run after sign-in in
  `qml/shipwright-shoal-messages.qml`.
- **Bridged rooms.** Mark rooms whose members include the bridge's ghost users
  (`@signal_*`, `@telegram_*`) or that carry `m.bridge` state:
  `core/src/roomlist.rs` adds a `bridge` field to each room entry,
  `src/roomlistmodel.{h,cpp}` exposes a role, `qml/pages/RoomDelegate.qml` and
  `qml/pages/RoomInfoPage.qml` show it. Next to the existing padlock, a bridged
  room must not look end-to-end encrypted end to end (trust model in the plan).
- **Privacy copy.** `qml/pages/PrivacyPage.qml` gains the section on bridged
  conversations, bridge credentials and retention, matching the published
  privacy policy.

## 2. Cover unread count and mark-all-read action

**Status 2026-10-01:** done (`rooms.markAllRead`, `core/src/readall.rs`, cover
action); decided: the action covers the whole account, like the count.
Needs device verification.

The count already exists: `qml/cover/CoverPage.qml` shows
`matrix.unreadRooms / matrix.unreadMessages` (`src/matrixbridge.h`, fed by
`src/roomlistmodel.cpp`). What is missing:

- **Cover action.** Add a `CoverActionList` with a mark-all-read action to
  `qml/cover/CoverPage.qml`, enabled only when signed in and
  `unreadRooms > 0`.
- **Mark all read.** New `Q_INVOKABLE void markAllRead()` in
  `src/matrixbridge.{h,cpp}`. The per-room path already exists:
  `MatrixBridge::markRoomRead` sends `room.markRead` (`core/src/protocol.rs`),
  used from `qml/pages/RoomListPage.qml`. Add a `rooms.markAllRead` command
  handled in `core/src/runtime.rs` that walks the room list with unread
  counts, reusing the same handler, bounded in concurrency, honouring the
  read-receipt setting (`sendReadReceipts` in `src/appsettings.h`: private
  receipt when public ones are off).
- **Space rollup.** The upstream README notes a space badge does not roll up
  sub-spaces (`src/spacemarkers.cpp`, `qml/pages/SpacesPage.qml`); decide
  whether the cover count should respect the current space.
- The cover itself is Qt5/Lipstick here; after the Keel port it goes through
  keel-shell's cover path (plan, "Covers").

## 3. Native contacts integration

**Status 2026-10-01:** opt-in matching done for the member list and profile
(`core/src/contacts.rs`, `src/contactsbridge.cpp`, `Contacts` permission).
Second pass the same day: contact names in the chat list (direct chats) and in
push banners; numbers without a country code read with the SIM's country from
oFono, else the locale's (`core/src/dialling.rs`, full MCC table in
`core/data/mcc-dialling.tsv`); a phone-contact picker that starts a Signal or
Telegram chat through the bridge's `start-chat` (Signal finds any number with
an account; Telegram only numbers its bridge has seen). Open: writing back.
Phone matching is limited on our cell because `phone_numbers_in_profile` is
off (README). All of it needs device verification.

- **Permission.** Add `Contacts` to both desktop files
  (`shipwright-shoal-messages.desktop` and `-open-url.desktop`;
  `scripts/desktop-check.py` enforces that they match). **[VERIFY]** the
  permission's scope on 5.2 from `/etc/sailjail/permissions/Contacts.permission`.
- **Reading contacts.** New `src/contactsbridge.{h,cpp}` on QtContacts (the
  `org.nemomobile.contacts` / qtcontacts-sqlite backend), added to `QT +=` in
  `shipwright-shoal-messages.pro` and `BuildRequires` in the spec. Matching is
  local only: phone numbers and Matrix IDs stored on the contact. No upload of
  the address book to any server.
- **Bridged identities.** For Signal and Telegram rooms, the bridge's ghost
  users carry the phone number or username; map them to contacts in
  `core/src/members.rs` output and show the contact name and avatar in
  `qml/pages/RoomDelegate.qml`, `qml/pages/MemberProfilePage.qml` and push
  banners (`src/pushwake.cpp`).
- **Starting chats.** `qml/pages/NewChatDialog.qml` gains a contact picker
  (Sailfish.Pickers has none for contacts; use a `SilicaListView` over the
  contacts model).
- **Writing back (later).** Optionally store the Matrix ID on the contact as
  an online account detail so the People app links to us.

## 4. Subscription lifecycle

The client side of Gate 2's "working subscription lifecycle". Server side:
`services/licence-service` and `reef/licence`.

- **Account creation.** Preset for the Shipwright homeserver on
  `qml/pages/LoginPage.qml`; registration and sign-in use the existing OAuth
  2.0/MAS and device-code flows in `core/src/login.rs` (the
  `login.registrationUrl` command already opens the server's registration).
  Replace the placeholder `CLIENT_URI` in `core/src/login.rs`.
- **Onboarding flow, replacing the stop-gap.** Today
  `qml/pages/SubscriptionPage.qml` (Account › Shipwright subscription) takes
  the whole onboarding bundle by paste or file and applies only its `push`
  section (`core/src/hosted.rs`, `push.configureHosted`). The real flow:
  purchase in Reef, the licence service provisions the account, and the app
  receives the bundle without the user copying secrets around (a one-time
  code or a link opened through the app's `matrix:`/https handler in
  `src/appservice.cpp`). It then uses the rest of the bundle: `matrix`
  (sign in with `login.password` or the MAS flow in `core/src/login.rs`,
  forcing a password change for `initial_password`) and `bridges` (item 1),
  and switches push on. Parsing moves from `hosted.rs` into a
  `bundle.rs` that keeps the Matrix password as a `Secret`.
- **Entitlement.** New `core/src/subscription.rs`: query the licence service
  with the Matrix access token or a licence token, cache the answer next to
  the session (`core/src/session.rs` keeps the encrypted session file;
  entitlement is not secret but should be tamper-evident), and emit
  `subscription.changed`. New `qml/pages/SubscriptionPage.qml` reached from
  `qml/pages/AccountPage.qml`: state, renewal date, manage via Reef.
- **States.** trial, active, grace, lapsed, cancelled. Lapsed: Matrix-native
  rooms keep working, bridged rooms show a banner (reuse
  `qml/pages/SendQueueBanner.qml` styling), the bridges are stopped server
  side. Cancelled: offer bridge logout (item 1) and data export before the
  account closes.
- **Push tie-in.** On activation offer push with Shoal Push and the Shipwright
  gateway (`qml/pages/PushPage.qml`, defaults already in
  `src/shoalconfig.h` and `core/src/push.rs`). On account deletion, the
  existing sign-out path already clears pushers and the registration
  (`core/src/runtime.rs`, sign-out).
- **Re-registration.** Upstream does not re-register when a distributor is
  reinstalled (`docs/PUSH.md`, Status). Add it in `core/src/push.rs`: on
  `push.status`, if push is enabled but `get_distributor()` came back `None`,
  run the enable path again.

## 5. Keel port (Qt6)

**Status 2026-10-01:** assessed, not started: `docs/KEEL-PORT.md`
recommends staying on system Qt 5 Silica until Keel's visual parity is
verified on a device.

The UI is Qt 5.6 plus Silica today. The port moves it onto Keel (the
Qt6 Sailfish.Silica layer, `keel/`), keeping the Rust core unchanged: the
FFI is plain C and JSON, so only `src/` and `qml/` move.

- **Build.** Replace `shipwright-shoal-messages.pro` with CMake against Qt6
  (plan, "Build"); link the same `libshoal_messages_core.a`. The version guard
  in the `.pro` (core stamp versus spec) moves into CMake.
- **Entry point.** `src/shipwright-shoal-messages.cpp` uses `SailfishApp::`
  (`application`, `createView`, `pathTo`); switch to `keel/sailfishapp`.
- **C++ bridge.** `src/*.cpp` is Qt5 API. The Qt Multimedia 5 classes are
  the main work: `QCamera` and the recorder (`src/camerasource.cpp`,
  `src/voicerecorder.cpp`) changed in Qt6, and `QAbstractVideoSurface`
  (`src/videostream.{h,cpp}`) is gone, replaced by `QVideoSink`. QtDBus is
  unchanged. The
  list models (`src/difflistmodel.cpp`, `src/roomlistmodel.cpp`,
  `src/timelinemodel.cpp`) port as they are.
- **QML imports** (from `qml/`): `Sailfish.Silica 1.0` (81 files, Keel),
  `QtQuick 2.0` (versionless in Qt6), `QtMultimedia 5.6` (6 files),
  `QtGraphicalEffects 1.0` (`qml/pages/Avatar.qml`, becomes
  `Qt5Compat.GraphicalEffects`), `QtDocGallery 5.0`
  (`qml/pages/AttachmentPickerPage.qml`, no Qt6 build: replace with a
  Tracker query in C++), `Qt.labs.folderlistmodel`, `QtSensors`,
  `Sailfish.Pickers`, `Sailfish.Gallery`, `Sailfish.Share`,
  `Nemo.Notifications 1.0`, `Nemo.KeepAlive 1.2`
  (`qml/shipwright-shoal-messages.qml`, `BackgroundJob`). The Nemo modules
  come from `keel/nemo-compat`; Pickers, Gallery and Share need Keel
  equivalents or replacements.
- **Scanner.** Run `tools/keel-compat` over `qml/` first; its report is the
  work list. The Qt5 build stays shippable until the Keel build reaches the
  same tier on device.
