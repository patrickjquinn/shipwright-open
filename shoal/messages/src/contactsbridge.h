// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#ifndef CONTACTSBRIDGE_H
#define CONTACTSBRIDGE_H

#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>

#include <functional>

class AppSettings;
class QDBusPendingCallWatcher;

namespace QtContacts {
class QContactManager;
class QContactFetchRequest;
}

/// Reads the device address book, read-only, and hands the core the fields
/// that can name a Matrix or bridged user (`contacts.setIndex`, matched in
/// `core/src/contacts.rs`). Opt-in: nothing is read while
/// `settings.contactsMatching` is off, and switching it off drops the core's
/// copy. The address book never leaves the device and is never written.
///
/// Backend: QtContacts with qtcontacts-sqlite (`org.nemomobile.contacts.sqlite`),
/// the store behind the People app. Its database is in the privileged data
/// directory, which the Sailjail `Contacts` permission opens.
///
/// Numbers saved without a country code are read with the dialling plan of
/// the SIM's country: its mobile country code comes from oFono on the system
/// bus (reachable through the `Internet` permission), the locale's country is
/// the fallback. The core maps either to a plan (`core/src/dialling.rs`).
class ContactsBridge : public QObject
{
    Q_OBJECT

    /// Address-book entries in the core's index; 0 while off or unreadable.
    Q_PROPERTY(int indexed READ indexed NOTIFY indexedChanged)
    /// "off", "reading", "ready" or "unavailable".
    Q_PROPERTY(QString status READ status NOTIFY indexedChanged)
    /// How numbers without a country code are read: `source` ("sim",
    /// "locale" or "none"), `region` ("DE") and `callingCode` ("49").
    Q_PROPERTY(QVariantMap dialling READ dialling NOTIFY indexedChanged)
    /// Contacts with phone numbers, one row per number (`name`, `number`,
    /// `mobile`), for the contact picker. Empty while matching is off.
    Q_PROPERTY(QVariantList phoneBook READ phoneBook NOTIFY phoneBookChanged)

public:
    using Sender = std::function<quint64(const QString &, const QJsonObject &, bool)>;

    explicit ContactsBridge(AppSettings *settings, QObject *parent = nullptr);
    ~ContactsBridge() override;

    void setSender(Sender sender) { m_send = std::move(sender); }

    int indexed() const { return m_indexed; }
    QString status() const { return m_status; }
    QVariantMap dialling() const { return m_dialling; }
    QVariantList phoneBook() const { return m_phoneBook; }

    /// Matching is on and the address book is being read: a push banner
    /// waits for the index, so it can name the sender.
    bool settling() const;

    /// Reads the address book again (or clears the index when off).
    Q_INVOKABLE void reload();
    /// The core answered `contacts.setIndex`.
    void reportIndexed(int count, const QVariantMap &dialling);
    /// Signed in again: the core starts without an index.
    void sessionStarted();

signals:
    void indexedChanged();
    void phoneBookChanged();

private:
    void fetchFinished();
    void setStatus(const QString &status, int indexed);
    void ensureManager();
    void dropManager();
    void queryCountryCode();
    void modemsListed(QDBusPendingCallWatcher *watcher, int generation);
    void askNextModem(int generation);
    void countryCodeKnown(const QString &mcc);
    void sendIndex();

    AppSettings *m_settings = nullptr;
    Sender m_send;
    QtContacts::QContactManager *m_manager = nullptr;
    QtContacts::QContactFetchRequest *m_fetch = nullptr;
    /// Changes in the address book arrive in bursts (a sync); one read after.
    QTimer m_debounce;
    int m_indexed = 0;
    QString m_status = QStringLiteral("off");
    QVariantMap m_dialling;
    QVariantList m_phoneBook;

    /// The entries read, until the country code is known as well.
    QJsonArray m_entries;
    bool m_entriesReady = false;
    /// The SIM's (or network's) mobile country code, "" where there is none.
    QString m_mcc;
    bool m_mccPending = false;
    /// oFono objects still to ask: (modem path, interface).
    QList<QPair<QString, QString>> m_modemQueue;
    /// Replies to an earlier query are ignored once a new one started.
    int m_mccGeneration = 0;
};

#endif // CONTACTSBRIDGE_H
