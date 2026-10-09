// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: Apache-2.0
//
// New file, added by Shipwright in 2026 (not part of upstream xmatic).
#include "bridgeactions.h"

#include "appsettings.h"

#include <QJsonArray>

namespace {

bool knownBridge(const QString &bridge)
{
    return bridge == QLatin1String("signal") || bridge == QLatin1String("telegram");
}

} // namespace

BridgeActions::BridgeActions(AppSettings *settings, QObject *parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QVariantMap BridgeActions::state(const QString &bridge) const
{
    QVariantMap value = m_states.value(bridge);
    if (!value.contains(QStringLiteral("health"))) {
        value.insert(QStringLiteral("health"), QStringLiteral("unknown"));
        value.insert(QStringLiteral("step"), QStringLiteral("idle"));
    }
    value.insert(QStringLiteral("bridge"), bridge);
    return value;
}

void BridgeActions::setField(const QString &bridge, const QString &key, const QVariant &value)
{
    m_states[bridge].insert(key, value);
}

void BridgeActions::request(const QString &bridge, const QString &command,
                            QJsonObject arguments, bool wipe)
{
    if (!knownBridge(bridge) || !m_send) {
        return;
    }
    arguments.insert(QStringLiteral("bridge"), bridge);
    // The bundle's bot where there was a bundle; empty lets the core assume the
    // bot on the account's own server.
    if (m_settings) {
        arguments.insert(QStringLiteral("bot"), m_settings->bridgeBot(bridge));
    }
    const quint64 id = m_send(command, arguments, wipe);
    if (id == 0) {
        return;
    }
    m_requests.insert(id, bridge);
    setField(bridge, QStringLiteral("busy"), true);
    setField(bridge, QStringLiteral("error"), QString());
    emit changed();
}

void BridgeActions::refresh(const QString &bridge)
{
    request(bridge, QStringLiteral("bridges.status"), QJsonObject());
}

void BridgeActions::refreshAll()
{
    refresh(QStringLiteral("signal"));
    refresh(QStringLiteral("telegram"));
}

void BridgeActions::link(const QString &bridge, const QString &flow)
{
    QJsonObject arguments;
    arguments.insert(QStringLiteral("flow"), flow);
    request(bridge, QStringLiteral("bridges.login"), arguments);
}

void BridgeActions::submit(const QString &bridge, const QString &value)
{
    if (value.trimmed().isEmpty()) {
        return;
    }
    QJsonObject arguments;
    arguments.insert(QStringLiteral("value"), value.trimmed());
    // A phone number, a login code or a two-factor password: not kept, not logged.
    request(bridge, QStringLiteral("bridges.submit"), arguments, true);
}

void BridgeActions::cancel(const QString &bridge)
{
    request(bridge, QStringLiteral("bridges.cancel"), QJsonObject());
}

void BridgeActions::unlink(const QString &bridge, const QString &loginId)
{
    QJsonObject arguments;
    arguments.insert(QStringLiteral("loginId"), loginId);
    request(bridge, QStringLiteral("bridges.logout"), arguments);
}

void BridgeActions::startChat(const QString &bridge, const QString &number)
{
    if (number.trimmed().isEmpty()) {
        return;
    }
    QJsonObject arguments;
    arguments.insert(QStringLiteral("number"), number.trimmed());
    // A phone number from the address book: not kept, not logged.
    request(bridge, QStringLiteral("bridges.startChat"), arguments, true);
}

void BridgeActions::reportChatReady(const QJsonObject &data)
{
    const QString bridge = data.value(QStringLiteral("bridge")).toString();
    if (!knownBridge(bridge)) {
        return;
    }
    const QString error = data.value(QStringLiteral("error")).toString();
    if (!error.isEmpty()) {
        setField(bridge, QStringLiteral("error"), error);
        emit changed();
        emit failed(bridge, error);
        return;
    }
    emit chatReady(bridge, data.value(QStringLiteral("roomId")).toString());
}

void BridgeActions::reportState(const QJsonObject &data)
{
    const QString bridge = data.value(QStringLiteral("bridge")).toString();
    if (!knownBridge(bridge)) {
        return;
    }
    QVariantMap next = data.toVariantMap();
    // What only this side knows survives the core's snapshot.
    const QVariantMap previous = m_states.value(bridge);
    next.insert(QStringLiteral("busy"), previous.value(QStringLiteral("busy")).toBool());
    next.insert(QStringLiteral("error"), previous.value(QStringLiteral("error")).toString());
    m_states.insert(bridge, next);
    emit changed();
}

bool BridgeActions::takeReply(quint64 id, bool ok, const QJsonObject &data, const QString &error)
{
    const auto found = m_requests.find(id);
    if (found == m_requests.end()) {
        return false;
    }
    const QString bridge = found.value();
    m_requests.erase(found);
    bool stillBusy = false;
    for (const QString &other : m_requests) {
        stillBusy = stillBusy || other == bridge;
    }
    if (ok && data.contains(QStringLiteral("step"))) {
        reportState(data);
    }
    setField(bridge, QStringLiteral("busy"), stillBusy);
    setField(bridge, QStringLiteral("error"), ok ? QString() : error);
    emit changed();
    if (!ok) {
        emit failed(bridge, error);
    }
    return true;
}

void BridgeActions::storeBots(const QJsonObject &bots)
{
    if (!m_settings) {
        return;
    }
    for (auto it = bots.constBegin(); it != bots.constEnd(); ++it) {
        if (knownBridge(it.key())) {
            m_settings->setBridgeBot(it.key(), it.value().toString());
        }
    }
}

void BridgeActions::reset()
{
    m_states.clear();
    m_requests.clear();
    emit changed();
}
