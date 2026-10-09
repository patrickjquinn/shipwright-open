// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#include "contactsbridge.h"

#include "appsettings.h"

#include <QContactAvatar>
#include <QContactDisplayLabel>
#include <QContactEmailAddress>
#include <QContactFetchHint>
#include <QContactFetchRequest>
#include <QContactManager>
#include <QContactName>
#include <QContactOnlineAccount>
#include <QContactPhoneNumber>
#include <QContactUrl>
#include <QCollator>
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusObjectPath>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QJsonArray>
#include <QLocale>
#include <QLoggingCategory>

#include <algorithm>

QTCONTACTS_USE_NAMESPACE

Q_LOGGING_CATEGORY(lcContacts, "shoal.messages.contacts")

namespace {

const char *const kManager = "org.nemomobile.contacts.sqlite";

const char *const kOfono = "org.ofono";
const char *const kOfonoSim = "org.ofono.SimManager";
const char *const kOfonoNetwork = "org.ofono.NetworkRegistration";
/// oFono answers at once or not at all; the index waits for this at most.
const int kOfonoTimeoutMs = 3000;

const int kDebounceMs = 3000;

/// A Matrix ID, or text that may hold one (the core decides).
bool mayBeMatrix(const QString &text)
{
    return text.contains(QLatin1Char('@')) && text.contains(QLatin1Char(':'));
}

/// The locale's country as ISO 3166 ("de_DE" -> "DE"); the core's fallback
/// when there is no SIM. Empty for a locale without a country ("C").
QString localeRegion()
{
    const QString name = QLocale::system().name();
    const int separator = name.indexOf(QLatin1Char('_'));
    return separator < 0 ? QString() : name.mid(separator + 1, 2).toUpper();
}

/// A mobile country code is three digits; anything else is no answer.
bool isCountryCode(const QString &mcc)
{
    if (mcc.size() != 3) {
        return false;
    }
    for (const QChar c : mcc) {
        if (!c.isDigit()) {
            return false;
        }
    }
    return true;
}

QJsonObject entry(const QContact &contact)
{
    QJsonObject object;
    object.insert(QStringLiteral("id"), contact.id().toString());
    QString name = contact.detail<QContactDisplayLabel>().label();
    if (name.isEmpty()) {
        const QContactName parts = contact.detail<QContactName>();
        name = QStringList({parts.firstName(), parts.lastName()}).join(QLatin1Char(' ')).trimmed();
    }
    object.insert(QStringLiteral("name"), name);
    object.insert(QStringLiteral("avatar"),
                  contact.detail<QContactAvatar>().imageUrl().toString());

    QJsonArray phones;
    for (const QContactPhoneNumber &phone : contact.details<QContactPhoneNumber>()) {
        phones.append(phone.number());
    }
    object.insert(QStringLiteral("phones"), phones);

    QJsonArray emails;
    for (const QContactEmailAddress &email : contact.details<QContactEmailAddress>()) {
        emails.append(email.emailAddress());
    }
    object.insert(QStringLiteral("emails"), emails);

    QJsonArray matrixIds;
    QJsonArray accounts;
    for (const QContactOnlineAccount &account : contact.details<QContactOnlineAccount>()) {
        QJsonObject item;
        item.insert(QStringLiteral("service"), account.serviceProvider());
        item.insert(QStringLiteral("handle"), account.accountUri());
        accounts.append(item);
    }
    for (const QContactUrl &url : contact.details<QContactUrl>()) {
        const QString text = url.url();
        if (text.startsWith(QLatin1String("matrix:")) || text.contains(QLatin1String("matrix.to/"))
            || mayBeMatrix(text)) {
            matrixIds.append(text);
        }
    }
    object.insert(QStringLiteral("matrixIds"), matrixIds);
    object.insert(QStringLiteral("accounts"), accounts);
    return object;
}

} // namespace

ContactsBridge::ContactsBridge(AppSettings *settings, QObject *parent)
    : QObject(parent)
    , m_settings(settings)
{
    m_debounce.setSingleShot(true);
    m_debounce.setInterval(kDebounceMs);
    connect(&m_debounce, &QTimer::timeout, this, &ContactsBridge::reload);
    if (m_settings) {
        connect(m_settings, &AppSettings::contactsMatchingChanged, this, &ContactsBridge::reload);
    }
}

ContactsBridge::~ContactsBridge() = default;

bool ContactsBridge::settling() const
{
    // Only a read in progress: a process that never got a session never reads
    // the book, and its banner must not wait for nothing.
    return m_settings && m_settings->contactsMatching() && m_status == QLatin1String("reading");
}

void ContactsBridge::setStatus(const QString &status, int indexed)
{
    if (status == m_status && indexed == m_indexed) {
        return;
    }
    m_status = status;
    m_indexed = indexed;
    emit indexedChanged();
}

void ContactsBridge::ensureManager()
{
    if (m_manager) {
        return;
    }
    m_manager = new QContactManager(QString::fromLatin1(kManager), QMap<QString, QString>(), this);
    // Read-only use, but a change in the book is a reason to read again.
    connect(m_manager, &QContactManager::dataChanged, &m_debounce,
            static_cast<void (QTimer::*)()>(&QTimer::start));
    connect(m_manager, &QContactManager::contactsAdded, &m_debounce,
            static_cast<void (QTimer::*)()>(&QTimer::start));
    connect(m_manager, &QContactManager::contactsChanged, &m_debounce,
            static_cast<void (QTimer::*)()>(&QTimer::start));
    connect(m_manager, &QContactManager::contactsRemoved, &m_debounce,
            static_cast<void (QTimer::*)()>(&QTimer::start));
}

void ContactsBridge::dropManager()
{
    // Review finding E-2: qtcontacts-sqlite finishes a request on its worker
    // thread, and this can run inside AppSettings' signal. The request is
    // cancelled and cut off from this object now; the manager, which owns it,
    // goes once control is back in the event loop.
    if (m_fetch) {
        disconnect(m_fetch, nullptr, this, nullptr);
        m_fetch->cancel();
    }
    if (m_manager) {
        disconnect(m_manager, nullptr, &m_debounce, nullptr);
        m_manager->deleteLater();
    }
    m_fetch = nullptr;
    m_manager = nullptr;
    m_debounce.stop();
}

void ContactsBridge::reload()
{
    const bool enabled = m_settings && m_settings->contactsMatching();
    if (!enabled) {
        // The manager goes too: no change notifications for an unused feature.
        dropManager();
        ++m_mccGeneration;
        m_mccPending = false;
        m_entries = QJsonArray();
        m_entriesReady = false;
        if (!m_phoneBook.isEmpty()) {
            m_phoneBook.clear();
            emit phoneBookChanged();
        }
        m_dialling.clear();
        if (m_send) {
            m_send(QStringLiteral("contacts.clear"), QJsonObject(), false);
        }
        setStatus(QStringLiteral("off"), 0);
        return;
    }
    ensureManager();
    if (m_manager->error() != QContactManager::NoError
        || m_manager->managerName() != QLatin1String(kManager)) {
        qCWarning(lcContacts, "the address book is not available (error %d)",
                  static_cast<int>(m_manager->error()));
        setStatus(QStringLiteral("unavailable"), 0);
        return;
    }
    if (m_fetch && m_fetch->isActive()) {
        return;
    }
    if (!m_fetch) {
        m_fetch = new QContactFetchRequest(m_manager);
        m_fetch->setManager(m_manager);
        QContactFetchHint hint;
        hint.setDetailTypesHint(QList<QContactDetail::DetailType>()
                                << QContactDetail::TypeDisplayLabel << QContactDetail::TypeName
                                << QContactDetail::TypePhoneNumber
                                << QContactDetail::TypeEmailAddress
                                << QContactDetail::TypeOnlineAccount << QContactDetail::TypeUrl
                                << QContactDetail::TypeAvatar);
        hint.setOptimizationHints(QContactFetchHint::NoRelationships
                                  | QContactFetchHint::NoActionPreferences
                                  | QContactFetchHint::NoBinaryBlobs);
        m_fetch->setFetchHint(hint);
        connect(m_fetch, &QContactAbstractRequest::stateChanged, this,
                [this](QContactAbstractRequest::State state) {
                    if (state == QContactAbstractRequest::FinishedState) {
                        fetchFinished();
                    }
                });
    }
    setStatus(QStringLiteral("reading"), m_indexed);
    m_entriesReady = false;
    // Asked every time: the SIM may have been swapped since the last read.
    queryCountryCode();
    if (!m_fetch->start()) {
        setStatus(QStringLiteral("unavailable"), 0);
    }
}

void ContactsBridge::queryCountryCode()
{
    const int generation = ++m_mccGeneration;
    m_mccPending = true;
    m_mcc.clear();
    m_modemQueue.clear();
    QDBusConnection bus = QDBusConnection::systemBus();
    if (!bus.isConnected()) {
        countryCodeKnown(QString());
        return;
    }
    const QDBusMessage call = QDBusMessage::createMethodCall(
        QString::fromLatin1(kOfono), QStringLiteral("/"), QStringLiteral("org.ofono.Manager"),
        QStringLiteral("GetModems"));
    auto *watcher = new QDBusPendingCallWatcher(bus.asyncCall(call, kOfonoTimeoutMs), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this, generation](QDBusPendingCallWatcher *finished) {
                modemsListed(finished, generation);
            });
}

void ContactsBridge::modemsListed(QDBusPendingCallWatcher *watcher, int generation)
{
    watcher->deleteLater();
    if (generation != m_mccGeneration) {
        return;
    }
    const QDBusMessage reply = watcher->reply();
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
        // No oFono (a tablet, the emulator) or not allowed: the locale decides.
        qCInfo(lcContacts, "no modem information; numbers are read by the locale");
        countryCodeKnown(QString());
        return;
    }
    // a(oa{sv}): every modem with its properties.
    QList<QPair<QString, QStringList>> modems;
    const QDBusArgument list = reply.arguments().first().value<QDBusArgument>();
    list.beginArray();
    while (!list.atEnd()) {
        QDBusObjectPath path;
        QVariantMap properties;
        list.beginStructure();
        list >> path >> properties;
        list.endStructure();
        modems.append(qMakePair(path.path(),
                                properties.value(QStringLiteral("Interfaces")).toStringList()));
    }
    list.endArray();
    // The SIM's home country first, on every modem (dual SIM); the network
    // the phone is registered on only where no SIM answers.
    for (const auto &modem : modems) {
        if (modem.second.contains(QString::fromLatin1(kOfonoSim))) {
            m_modemQueue.append(qMakePair(modem.first, QString::fromLatin1(kOfonoSim)));
        }
    }
    for (const auto &modem : modems) {
        if (modem.second.contains(QString::fromLatin1(kOfonoNetwork))) {
            m_modemQueue.append(qMakePair(modem.first, QString::fromLatin1(kOfonoNetwork)));
        }
    }
    askNextModem(generation);
}

void ContactsBridge::askNextModem(int generation)
{
    if (generation != m_mccGeneration) {
        return;
    }
    if (m_modemQueue.isEmpty()) {
        countryCodeKnown(QString());
        return;
    }
    const QPair<QString, QString> next = m_modemQueue.takeFirst();
    const QDBusMessage call = QDBusMessage::createMethodCall(
        QString::fromLatin1(kOfono), next.first, next.second, QStringLiteral("GetProperties"));
    auto *watcher = new QDBusPendingCallWatcher(
        QDBusConnection::systemBus().asyncCall(call, kOfonoTimeoutMs), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [this, generation](QDBusPendingCallWatcher *finished) {
                finished->deleteLater();
                if (generation != m_mccGeneration) {
                    return;
                }
                const QDBusPendingReply<QVariantMap> reply = *finished;
                const QString mcc = reply.isError()
                    ? QString()
                    : reply.value().value(QStringLiteral("MobileCountryCode")).toString();
                if (isCountryCode(mcc)) {
                    countryCodeKnown(mcc);
                } else {
                    askNextModem(generation);
                }
            });
}

void ContactsBridge::countryCodeKnown(const QString &mcc)
{
    m_mcc = mcc;
    m_mccPending = false;
    sendIndex();
}

void ContactsBridge::fetchFinished()
{
    if (!m_fetch || !m_settings || !m_settings->contactsMatching()) {
        return;
    }
    if (m_fetch->error() != QContactManager::NoError) {
        qCWarning(lcContacts, "reading the address book failed (error %d)",
                  static_cast<int>(m_fetch->error()));
        setStatus(QStringLiteral("unavailable"), 0);
        return;
    }
    QJsonArray list;
    QVariantList phoneBook;
    for (const QContact &contact : m_fetch->contacts()) {
        const QJsonObject object = entry(contact);
        const QJsonArray phones = object.value(QStringLiteral("phones")).toArray();
        const bool useful = !phones.isEmpty()
            || !object.value(QStringLiteral("emails")).toArray().isEmpty()
            || !object.value(QStringLiteral("matrixIds")).toArray().isEmpty()
            || !object.value(QStringLiteral("accounts")).toArray().isEmpty();
        if (useful) {
            list.append(object);
        }
        const QString name = object.value(QStringLiteral("name")).toString();
        QStringList seen;
        for (const QContactPhoneNumber &phone : contact.details<QContactPhoneNumber>()) {
            const QString number = phone.number().trimmed();
            if (number.isEmpty() || seen.contains(number)) {
                continue;
            }
            seen.append(number);
            QVariantMap row;
            row.insert(QStringLiteral("name"), name.isEmpty() ? number : name);
            row.insert(QStringLiteral("number"), number);
            row.insert(QStringLiteral("mobile"),
                       phone.subTypes().contains(QContactPhoneNumber::SubTypeMobile));
            phoneBook.append(row);
        }
    }
    QCollator collator;
    collator.setCaseSensitivity(Qt::CaseInsensitive);
    std::sort(phoneBook.begin(), phoneBook.end(),
              [&collator](const QVariant &a, const QVariant &b) {
                  return collator.compare(a.toMap().value(QStringLiteral("name")).toString(),
                                          b.toMap().value(QStringLiteral("name")).toString())
                      < 0;
              });
    m_phoneBook = phoneBook;
    emit phoneBookChanged();
    m_entries = list;
    m_entriesReady = true;
    sendIndex();
}

void ContactsBridge::sendIndex()
{
    if (!m_entriesReady || m_mccPending) {
        return;
    }
    QJsonObject arguments;
    arguments.insert(QStringLiteral("contacts"), m_entries);
    arguments.insert(QStringLiteral("mcc"), m_mcc);
    arguments.insert(QStringLiteral("region"), localeRegion());
    // Counted, never listed: the journal is no place for an address book.
    qCInfo(lcContacts, "%d address-book entries handed to the core (SIM country %s)",
           m_entries.size(), m_mcc.isEmpty() ? "unknown" : "known");
    m_entries = QJsonArray();
    m_entriesReady = false;
    if (m_send) {
        m_send(QStringLiteral("contacts.setIndex"), arguments, true);
    }
}

void ContactsBridge::reportIndexed(int count, const QVariantMap &dialling)
{
    if (m_settings && m_settings->contactsMatching()) {
        const bool sameIndex = m_status == QLatin1String("ready") && m_indexed == count;
        m_dialling = dialling;
        if (sameIndex) {
            emit indexedChanged();
        } else {
            setStatus(QStringLiteral("ready"), count);
        }
    }
}

void ContactsBridge::sessionStarted()
{
    if (m_settings && m_settings->contactsMatching()) {
        reload();
    }
}
