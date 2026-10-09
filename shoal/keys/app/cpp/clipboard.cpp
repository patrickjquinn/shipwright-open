// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

#include "shipwright-shoal-keys/cpp/clipboard.h"

#include <QtCore/QMimeData>
#include <QtGui/QClipboard>
#include <QtGui/QGuiApplication>

void keys_clipboard_set(const QString& text)
{
    QClipboard* cb = QGuiApplication::clipboard();
    if (!cb)
        return;
    auto* mime = new QMimeData;
    mime->setText(text);
    // Clipboard managers that honour it (KDE Klipper, and others) keep the
    // value out of their history.
    mime->setData(QStringLiteral("x-kde-passwordManagerHint"), QByteArrayLiteral("secret"));
    cb->setMimeData(mime); // takes ownership
}

bool keys_clipboard_clear_if(const QString& text)
{
    QClipboard* cb = QGuiApplication::clipboard();
    if (!cb || cb->text() != text)
        return false; // the user copied something else since; leave it
    cb->clear();
    return true;
}
