<!-- SPDX-FileCopyrightText: 2026 Patrick Quinn -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Porting Shoal Messages to Keel (Qt 6): assessment

Written 2026-10-01 for ROADMAP.md item 5. Nothing was ported. Measured with
`tools/keel-compat` (`keel-compat --exclude core shoal/messages`), a grep of
`src/` for Qt 5-only classes, and `keel/silica/COMPATIBILITY.md` as of today.

## Where it stands

Shoal Messages runs on the system Qt 5.6 and Jolla's own Sailfish Silica.
That is as native as a Sailfish app gets: the real Silica components, the real
Lipstick cover, the system pickers, gallery and share sheet, all rendered by the
code that the stock apps use. Keel is a Qt 6 reimplementation of Silica's public
API. Its pixel-level parity with Silica (tier C) has **not** been checked on a
device (keel/README.md, "Needs device verification").

The protocol core (`core/`, Rust, plain C FFI with JSON) does not depend on Qt
at all and moves unchanged. Everything below concerns `qml/` and `src/`.

keel-compat: 84 QML, 80 native, 1 build and 2 desktop files scanned; 88 % of
distinct items reachable with Keel; tier A reached, **tier B blocked** by 7
items.

## What a Keel port needs

### QML modules (imports)

| Import | Files | Keel / Qt 6 status | Work |
|---|---|---|---|
| `Sailfish.Silica 1.0` | 84 | Keel 0.1 | See "Silica types" below |
| `QtQuick 2.0` / `2.5` | 84 | Qt 6 | None (versioned imports accepted) |
| `Nemo.Notifications 1.0`, `Nemo.KeepAlive 1.2` | `shipwright-shoal-messages.qml` | `keel/nemo-compat` | None expected; `BackgroundJob` timing needs device test |
| `QtMultimedia 5.6` | `shipwright-shoal-messages.qml`, `RoomPage`, `CameraCapturePage`, `VideoPage`, `CallPage`, `AttachmentPickerPage` | Qt 6 Multimedia API changed. `keel/qt5compat` shims `MediaPlayer`, `Audio`, `VideoOutput { source }`, `SoundEffect` | `Camera` (`CameraCapturePage`, `AttachmentPickerPage`) is shimmed too (on `CaptureSession` + `Camera` + `ImageCapture`), except `camera.metaData` (accepted, not passed on: no EXIF orientation from `pictureRotation`) and focus lock (reported locked at once); whether Qt 6 sees the device's cameras is unverified. `VideoOutput` fed from C++ (`src/videostream`, calls) must take a `QVideoSink` |
| `QtGraphicalEffects 1.0` | `Avatar.qml` | `keel/qt5compat` re-exports `Qt5Compat.GraphicalEffects` | Needs Chum `qt6-qt5compat` on the device |
| `Sailfish.Pickers 1.0` | `AccountPage`, `RoomPage`, `SubscriptionPage`, `RoomSettingsPage` | **missing** in Keel (out of v1 scope) | Keel needs `FilePickerPage`, `ImagePickerPage` (or we write our own on `Qt.labs.folderlistmodel` / Tracker) |
| `Sailfish.Gallery 1.0` | `AttachmentPickerPage` | **missing** | Replace the viewer component |
| `QtDocGallery 5.0` | `AttachmentPickerPage` | **no Qt 6 build exists** | Replace with a Tracker query in C++ (also works on Qt 5, so it can be done before the port) |
| `Qt.labs.folderlistmodel 2.1` | `EmojiFolderPage`, `AttachmentPickerPage` | In Qt 6 qtdeclarative | Check Chum's Qt 6 package ships the plugin **[VERIFY]** |
| `QtSensors 5.0` | `CameraCapturePage` | Qt 6 has QtSensors | Check Chum packages it and the Sailfish sensor backend exists for Qt 6 **[VERIFY]** |
| `Sailfish.Share 1.0` | `shipwright-shoal-messages.qml`, `ImageViewPage` | partial: `ShareAction.trigger()` copies to the clipboard; no share sheet | Real share UI needed in Keel, or accept the regression |

### Silica types beyond `keel/silica/COMPATIBILITY.md`

Every Silica type the app uses exists in Keel 0.1 (ApplicationWindow, Page,
Dialog, PullDownMenu, ContextMenu, ListItem, ComboBox, Remorse, SearchField,
PasswordField, Slider, CoverActionList, ...). The gaps are behaviour, not names:

- `Label` (282 uses): `TruncationMode.Fade` is drawn as elision (no
  OpacityRampEffect shader); `palette` unsupported. Visual only.
- `Screen.topCutout` (3 uses: `SecurityStatusPage`, `RoomPage`,
  `CameraCapturePage`): undefined on Qt 6 unless written `S.Screen` with
  `import Sailfish.Silica 1.0 as S`. One-line source change each; harmless on
  Qt 5, so it can be done now.
- `Cover`: Keel reports only Active/Inactive status and always `Cover.Large`;
  the new mark-all-read `CoverAction` goes through keel-shell's forwarded taps.
- `Clipboard`, `Format`, `StandardPaths`, `Theme`: implemented; `Theme`
  values come from keel-shell's forwarded ambience, not the system's.

### C++ (`src/`), Qt 5-only APIs and Qt 5 libraries

| Area | Files | Qt 6 situation | Work |
|---|---|---|---|
| Entry point | `shipwright-shoal-messages.cpp`, `voicetranscripts.cpp` (`SailfishApp::application/createView/pathTo`) | `keel/sailfishapp` provides them | Link Keel's; launch through keel-shell |
| Build | `shipwright-shoal-messages.pro` (`CONFIG += sailfishapp sailfishapp_i18n`) | qmake features not provided by Keel | CMake project against Qt 6; move the core-version guard and the qml/desktop checks into CMake |
| Video frames | `videostream.{h,cpp}`, `camerasource.{h,cpp}` (`QAbstractVideoSurface`), `callengine.cpp` (`QVideoFrame`) | `QAbstractVideoSurface` removed; `QVideoFrame` mapping and pixel formats changed | Rewrite on `QVideoSink` / `QVideoFrame` (Qt 6) |
| Camera | `camerasource.cpp` (`QCamera`) | API rewritten in Qt 6 (`QMediaCaptureSession`) | Rewrite; also needs the Sailfish camera backend for Qt 6 multimedia **[VERIFY]** |
| Voice recording | `voicerecorder.{h,cpp}` (`QAudioRecorder`, `QMediaRecorder`, `QAudioProbe`) | `QAudioRecorder` and `QAudioProbe` removed | `QMediaCaptureSession` + `QMediaRecorder`; level metering from a `QAudioSource` |
| Store key | `secretskeeper.cpp` (`Sailfish::Secrets`, libsailfishsecrets) | The client library is Qt 5 only | Rebuild sailfish-secrets' client library against Qt 6 (open source) or speak its peer-to-peer D-Bus protocol directly. **Critical**: this key decrypts the session and stores; a mistake locks users out |
| Address book | `contactsbridge.{h,cpp}` (QtContacts / QtPim, qtcontacts-sqlite backend, new today) | No supported Qt 6 QtPim on Sailfish; qtcontacts-sqlite is built for Qt 5 | Qt 6 builds of qtpim and qtcontacts-sqlite (both open source) or a different access path **[VERIFY]**; until then contact matching is Qt 5 only |
| Position | `locationactions.cpp` (`QGeoPositionInfoSource`) | QtPositioning exists in Qt 6 | Needs a Sailfish positioning plugin for Qt 6 **[VERIFY]** |
| Unchanged | QtDBus, QtConcurrent, `QQuickImageProvider`, list models, GStreamer (`callengine`, `voicedecode`), PulseAudio (`callaudiorouter`) | Port as they are | Compile and fix the usual Qt 6 source breaks |

### Platform integration

- **Cover:** Lipstick cover today; keel-shell's cover window per
  `keel/shell/PROTOCOL.md` after the port.
- **Push wake-up:** D-Bus activation through sailjail with
  `SHOAL_MESSAGES_PUSH_WAKE=1` runs the app headless (`src/pushwake.cpp`).
  Under keel-shell the app is a nested-compositor client; a headless start
  must still work without a window **[VERIFY]**.
- **Share target and link handler:** the two desktop files and
  `org.shipwright.ShoalMessages.service` must keep working when the binary is
  started by keel-shell.
- **Sailjail:** keel-shell and the Qt 6 libraries from Chum have to be
  readable inside the app's sandbox profile.

## Recommendation

**Keep Shoal Messages on the system Qt 5 Silica for now; do not port until
Keel's visual parity has been verified on a device.**

1. On Qt 5 the app *is* native, today, with nothing to verify. On Keel it
   would at best look like Silica: tier C is unmeasured, and Label fades,
   pickers, gallery and the share sheet are known regressions.
2. What we sell (bridges, push, subscription) lives in the Rust core and a
   thin C++ layer, none of which needs Qt 6. The Phase 2 work done so far
   (cover action, bridges onboarding, contact matching) shipped on Qt 5.
3. The hard parts of the port are not QML: Qt Multimedia's rewrite (camera,
   calls, voice messages) and Sailfish Secrets on Qt 6. The Secrets one carries
   real risk: a broken store key means users cannot open their messages.
4. Contact matching (added today) depends on QtPim and qtcontacts-sqlite,
   which have no Qt 6 build on Sailfish; porting now would drop it.

Cheap steps now that shorten the port later and are safe on Qt 5:

- Qualify the three `Screen.topCutout` uses (`S.Screen`).
- Replace `QtDocGallery` in `AttachmentPickerPage` with a Tracker query in
  C++ (works on both).
- Keep new C++ free of Qt 5-only classes, and isolate the ones that cannot be
  avoided behind one class each (as `ContactsBridge` is).
- Run `keel-compat --exclude core shoal/messages` in CI and track the score
  (88 % today, 7 blockers).

Revisit when: Keel tier C passes on a Jolla device, Keel or Chum provides
Pickers and a share sheet, and Qt 6 builds of the Secrets client library and
QtPim/qtcontacts-sqlite exist. Then start with a Keel build that keeps the Qt 5
build shippable (ROADMAP.md item 5) and switch only when both reach the same
tier on a device.
