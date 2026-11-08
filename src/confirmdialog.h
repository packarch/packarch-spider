/*
 * Confirmation dialogs with an optional "Do not ask again" checkbox.
 * Replaces KMessageBox::warningContinueCancel.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef CONFIRMDIALOG_H
#define CONFIRMDIALOG_H

#include <QString>

class QWidget;

namespace ConfirmDialog
{
// Returns true if the user chose to continue. If @p dontAskAgainName is not
// empty, the dialog offers a "Do not ask again" checkbox and the choice is
// remembered in the settings under that name.
bool warningContinueCancel(QWidget *parent,
                           const QString &text,
                           const QString &title,
                           const QString &continueText,
                           const QString &dontAskAgainName = QString());
}

#endif
