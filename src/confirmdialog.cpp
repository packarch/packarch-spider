/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "confirmdialog.h"

#include "i18n.h"
#include "settings.h"

#include <QCheckBox>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>

namespace
{
const QString notificationGroup = QStringLiteral("Notification Messages/");
}

bool ConfirmDialog::warningContinueCancel(QWidget *parent,
                                          const QString &text,
                                          const QString &title,
                                          const QString &continueText,
                                          const QString &dontAskAgainName)
{
    QSettings &config = Settings::config();
    if (!dontAskAgainName.isEmpty() && !config.value(notificationGroup + dontAskAgainName, true).toBool())
        return true;

    QMessageBox box(QMessageBox::Warning, title, text, QMessageBox::NoButton, parent);
    QPushButton *continueButton = box.addButton(continueText, QMessageBox::AcceptRole);
    box.addButton(QMessageBox::Cancel);
    box.setDefaultButton(continueButton);

    QCheckBox *checkBox = nullptr;
    if (!dontAskAgainName.isEmpty()) {
        checkBox = new QCheckBox(i18nc("@option:check", "Do not ask again"), &box);
        box.setCheckBox(checkBox);
    }

    box.exec();
    const bool accepted = box.clickedButton() == continueButton;

    if (accepted && checkBox && checkBox->isChecked()) {
        config.setValue(notificationGroup + dontAskAgainName, false);
        config.sync();
    }
    return accepted;
}
