// Modified by Shipwright, 2026: rebranded as Shoal Messages; see CHANGES-FROM-UPSTREAM.md.
// The store key in a Sailfish Secrets collection: 32 random bytes, device-lock
// bound, owner-only. A local key for the stores, not an account credential.

#include "secretskeeper.h"

#include <fcntl.h>
#include <unistd.h>

#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusReply>
#include <QDir>
#include <QFile>
#include <QFileInfo>

#include <Sailfish/Secrets/collectionnamesrequest.h>
#include <Sailfish/Secrets/healthcheckrequest.h>
#include <Sailfish/Secrets/lockcoderequest.h>
#include <Sailfish/Secrets/result.h>
#include <Sailfish/Secrets/secret.h>
#include <Sailfish/Secrets/secretmanager.h>
#include <Sailfish/Secrets/storedsecretrequest.h>

using namespace Sailfish::Secrets;

namespace {

const QString collectionName = QStringLiteral("ShoalMessages");
const QString secretName = QStringLiteral("storeKey");

Secret::Identifier keyIdentifier()
{
    return Secret::Identifier(secretName,
                              collectionName,
                              SecretManager::DefaultEncryptedStoragePluginName);
}

/// Overwrites a buffer so the write survives the optimiser. Only on an unshared
/// one: `data()` detaches and would wipe a fresh copy instead.
void wipe(QByteArray &bytes)
{
    if (bytes.isEmpty()) {
        return;
    }
    volatile char *raw = bytes.data();
    for (int i = 0; i < bytes.size(); ++i) {
        raw[i] = '\0';
    }
}

QByteArray randomKey()
{
    // /dev/urandom rather than qrand: Qt 5.6 has no QRandomGenerator, and a
    // store key must never come from a seedable PRNG.
    QFile urandom(QStringLiteral("/dev/urandom"));
    if (!urandom.open(QIODevice::ReadOnly)) {
        return QByteArray();
    }
    QByteArray key = urandom.read(32);
    return key.size() == 32 ? key : QByteArray();
}

/// The store key, 32 raw bytes, in the app's own data directory. Sailjail
/// gives no other app (sandboxed or not, short of root) that directory, and
/// it lies on the phone's encrypted home partition.
QString keyFilePath(const QString &dataDirectory)
{
    return dataDirectory + QStringLiteral("/store.key");
}

/// The key file's 32 bytes, or empty if it is missing or not a key.
QByteArray readKeyFile(const QString &path)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return QByteArray();
    }
    QByteArray data = file.read(33);
    if (data.size() != 32) {
        wipe(data);
        return QByteArray();
    }
    return data;
}

/// Writes the key file atomically: a new file, owner-only from the start,
/// synced, then renamed over the old one.
bool writeKeyFile(const QString &path, const QByteArray &key)
{
    QDir().mkpath(QFileInfo(path).absolutePath());
    const QByteArray target = QFile::encodeName(path);
    const QByteArray temporary = target + ".new";
    ::unlink(temporary.constData());
    const int fd = ::open(temporary.constData(), O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0600);
    if (fd < 0) {
        return false;
    }
    bool ok = ::write(fd, key.constData(), static_cast<size_t>(key.size())) == key.size();
    ok = ::fsync(fd) == 0 && ok;
    ok = ::close(fd) == 0 && ok;
    if (!ok || ::rename(temporary.constData(), target.constData()) != 0) {
        ::unlink(temporary.constData());
        return false;
    }
    return true;
}

/// An `Available` result for `key` (raw), which is wiped here.
StoreKeyResult available(QByteArray &key)
{
    // The base64 is wiped too: `toBase64()` builds a second buffer holding the
    // same key in another alphabet.
    QByteArray encoded64 = key.toBase64();
    StoreKeyResult outcome;
    outcome.state = StoreKeyState::Available;
    outcome.key = QString::fromLatin1(encoded64);
    wipe(encoded64);
    wipe(key);
    return outcome;
}

} // namespace

bool encryptedDataPresent(const QString &dataDirectory)
{
    if (QFile::exists(dataDirectory + QStringLiteral("/store/.encrypted"))) {
        return true;
    }
    // The encrypted lists can be the only thing on disk - allowed a caller, then
    // signed out. Minting a fresh key over them makes them unreadable for good.
    if (QFile::exists(dataDirectory + QStringLiteral("/private.json"))) {
        return true;
    }
    QFile session(dataDirectory + QStringLiteral("/session.json"));
    if (!session.exists()) {
        return false;
    }
    if (!session.open(QIODevice::ReadOnly)) {
        // There and unreadable is not "nothing is encrypted": this answer decides
        // whether a new key goes over the old one, so it fails towards doing nothing.
        return true;
    }
    // The envelope starts with the core's one top-level key, a plaintext session
    // with the homeserver. Only the shape is looked at.
    QByteArray head = session.read(64);
    const bool envelope = head.contains("\"encrypted\"");
    wipe(head);
    return envelope;
}

bool localDataPresent(const QString &dataDirectory)
{
    if (QFile::exists(dataDirectory + QStringLiteral("/session.json"))
        || QFile::exists(dataDirectory + QStringLiteral("/private.json"))) {
        return true;
    }
    QDir store(dataDirectory + QStringLiteral("/store"));
    if (!store.exists()) {
        return false;
    }
    // The core's `prepare()` creates this directory, so "exists and empty" is a
    // fresh install. Hidden entries count - `.encrypted` is one.
    return !store.entryList(QDir::AllEntries | QDir::NoDotAndDotDot | QDir::Hidden)
                .isEmpty();
}

bool secretsDaemonPresent()
{
    // Asked of the bus, not the filesystem: inside Sailjail the app sees neither
    // the binary nor the service file even where both are installed.
    const QDBusConnection bus = QDBusConnection::sessionBus();
    if (bus.isConnected()) {
        if (QDBusConnectionInterface *iface = bus.interface()) {
            const QString name =
                QStringLiteral("org.sailfishos.secrets.daemon.discovery");
            if (iface->isServiceRegistered(name).value()) {
                return true;
            }
            // `ListActivatableNames` by hand - Qt 5.6 has no accessor. Bounded: this runs
            // before the window is up and a silent bus must not hold the app.
            QDBusMessage call = QDBusMessage::createMethodCall(
                QStringLiteral("org.freedesktop.DBus"),
                QStringLiteral("/org/freedesktop/DBus"),
                QStringLiteral("org.freedesktop.DBus"),
                QStringLiteral("ListActivatableNames"));
            const QDBusMessage reply = bus.call(call, QDBus::Block, 2000);
            if (reply.type() == QDBusMessage::ReplyMessage) {
                return reply.arguments().value(0).toStringList().contains(name);
            }
            // The bus is there but would not answer. Fall through to the files
            // rather than claim anything.
        }
    }
    // No bus to ask, so fall back to the files. The doubtful answer is "it is
    // there": sending somebody to install what they have is the worse mistake.
    return QFile::exists(QStringLiteral("/usr/bin/sailfishsecretsd"))
           || QFile::exists(QStringLiteral(
                  "/usr/share/dbus-1/services/"
                  "org.sailfishos.secrets.daemon.discovery.service"));
}

namespace {

/// Fills in the reason from a request result, for the journal and the UI.
StoreKeyResult failure(StoreKeyState state, const Result &result)
{
    StoreKeyResult outcome;
    outcome.state = state;
    outcome.errorCode = static_cast<int>(result.errorCode());
    // secretsd's own text. It names collections and plugins, never key
    // material — the same string this app has logged since 0.18.1.
    outcome.errorMessage = result.errorMessage();
    return outcome;
}

} // namespace

SecretsDiagnosis inspectSecrets()
{
    SecretsDiagnosis outcome;
    SecretManager manager;

    // The first failure only: the page shows one line.
    const auto note = [&outcome](const Result &result) {
        if (outcome.errorCode == 0) {
            outcome.errorCode = static_cast<int>(result.errorCode());
            outcome.errorMessage = result.errorMessage();
        }
    };

    // `PreventInteraction`: this runs while a page is built.
    {
        LockCodeRequest lock;
        lock.setManager(&manager);
        lock.setLockCodeRequestType(LockCodeRequest::QueryLockStatus);
        lock.setLockCodeTargetType(LockCodeRequest::MetadataDatabase);
        lock.setUserInteractionMode(SecretManager::PreventInteraction);
        lock.startRequest();
        lock.waitForFinished();
        if (lock.result().code() == Result::Succeeded) {
            outcome.lockStatus = static_cast<int>(lock.lockStatus());
        } else {
            note(lock.result());
        }
    }

    // Corrupted is not locked, and needs other advice.
    {
        HealthCheckRequest health;
        health.setManager(&manager);
        health.startRequest();
        health.waitForFinished();
        if (health.result().code() == Result::Succeeded) {
            outcome.masterlockHealth = static_cast<int>(health.masterlockHealth());
            outcome.saltDataHealth = static_cast<int>(health.saltDataHealth());
        } else {
            note(health.result());
        }
    }

    // Whether it is only this app's collection.
    {
        CollectionNamesRequest names;
        names.setManager(&manager);
        names.setStoragePluginName(SecretManager::DefaultEncryptedStoragePluginName);
        names.startRequest();
        names.waitForFinished();
        if (names.result().code() == Result::Succeeded) {
            outcome.collection = names.collectionNames().contains(collectionName)
                                     ? (names.isCollectionLocked(collectionName) ? 1 : 0)
                                     : 2;
        } else {
            note(names.result());
        }
    }

    qInfo("shoal-messages: secrets diagnosis - metadata lock %d, masterlock health %d, "
          "salt health %d, own collection %d (%d: %s)",
          outcome.lockStatus,
          outcome.masterlockHealth,
          outcome.saltDataHealth,
          outcome.collection,
          outcome.errorCode,
          qPrintable(outcome.errorMessage));
    return outcome;
}

StoreKeyResult obtainStoreKey(const QString &dataDirectory)
{
    const QString path = keyFilePath(dataDirectory);

    // The key file first: it is where every key lives since 0.1.0-8.
    {
        QByteArray data = readKeyFile(path);
        if (data.size() == 32) {
            qInfo("shoal-messages: store key loaded from the app's private storage");
            return available(data);
        }
    }

    // A key from before 0.1.0-8 lives in Sailfish Secrets. Read without
    // interaction: on Sailfish OS 5.2 the device-lock check a locked
    // collection needs cannot run for a sandboxed app (polkit has no agent
    // for it, and secretsd's device-lock fallback shows no dialog), so a
    // dialog would only ever time out. The collection is open in the boot
    // it was created in, and locked for good after the next restart.
    SecretManager manager;
    StoredSecretRequest read;
    read.setManager(&manager);
    read.setIdentifier(keyIdentifier());
    read.setUserInteractionMode(SecretManager::PreventInteraction);
    read.startRequest();
    read.waitForFinished();
    const Result result = read.result();
    if (result.code() == Result::Succeeded) {
        // Detached before anything else: `secret().data()` shares with the
        // request's own buffer, and wiping a shared one wipes a copy.
        QByteArray data = read.secret().data();
        data.detach();
        if (data.size() == 32) {
            // Into the key file while it can still be read, so the next
            // restart does not lock the data away.
            if (writeKeyFile(path, data)) {
                qInfo("shoal-messages: store key moved from the device's secrets storage "
                      "to the app's private storage");
            } else {
                qWarning("shoal-messages: store key read from the device's secrets storage "
                         "but not saved to %s; it is lost at the next restart",
                         qPrintable(path));
            }
            return available(data);
        }
        // A key of the wrong size cannot have encrypted anything.
        wipe(data);
        qWarning("shoal-messages: stored key has the wrong size, creating a new one");
    } else if (encryptedDataPresent(dataDirectory)) {
        // Something on disk was written under a key this read did not deliver.
        // Minting now would replace the key that data needs; the core answers
        // `locked`.
        qWarning("shoal-messages: store key not available (%d: %s); encrypted data waits for it",
                 static_cast<int>(result.errorCode()),
                 qPrintable(result.errorMessage()));
        // Locked even where the daemon is missing: this state is what forbids
        // minting a key, and that rule may not depend on why the read failed.
        return failure(StoreKeyState::Locked, result);
    }

    // First run: a new key, in the key file only.
    QByteArray key = randomKey();
    if (key.isEmpty()) {
        qWarning("shoal-messages: no randomness source; stores stay unencrypted");
        StoreKeyResult outcome;
        outcome.state = StoreKeyState::Unavailable;
        outcome.errorMessage = QStringLiteral("no randomness source");
        return outcome;
    }
    if (!writeKeyFile(path, key)) {
        wipe(key);
        qWarning("shoal-messages: the store key could not be saved to %s", qPrintable(path));
        StoreKeyResult outcome;
        outcome.state = StoreKeyState::Unavailable;
        outcome.errorMessage = QStringLiteral("the key file could not be written");
        return outcome;
    }
    qInfo("shoal-messages: store key created in the app's private storage");
    return available(key);
}
