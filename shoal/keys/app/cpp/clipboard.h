// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

// Clipboard access for the Keys object. QML has no clipboard API, and
// cxx-qt-lib does not wrap QClipboard, so these two calls are C++.
#pragma once

#include <QtCore/QString>

void keys_clipboard_set(const QString& text);
bool keys_clipboard_clear_if(const QString& text);
