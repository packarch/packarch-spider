/*
 * Appearance settings: card deck and game theme selection.
 * Replaces KConfigDialog + KCardThemeWidget page + KGameThemeSelector.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef APPEARANCEDIALOG_H
#define APPEARANCEDIALOG_H

#include <QDialog>

class KCardThemeWidget;
class QDialogButtonBox;
class QListWidget;

class AppearanceDialog : public QDialog
{
    Q_OBJECT

public:
    explicit AppearanceDialog(QWidget *parent = nullptr);

Q_SIGNALS:
    // Emitted after the new settings have been stored.
    void settingsChanged();

private:
    void loadSettings();
    void applySettings();
    void updateButtons();

    KCardThemeWidget *m_cardThemeWidget;
    QListWidget *m_themeList;
    QDialogButtonBox *m_buttonBox;
};

#endif
