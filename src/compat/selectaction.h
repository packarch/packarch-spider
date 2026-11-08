/*
 * An action showing a submenu of mutually exclusive, checkable choices.
 * Replaces KSelectAction (only the subset used by kpat).
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_SELECTACTION_H
#define KPAT_COMPAT_SELECTACTION_H

#include <QAction>

class QActionGroup;
class QMenu;

class SelectAction : public QAction
{
    Q_OBJECT

public:
    explicit SelectAction(const QString &text, QObject *parent = nullptr);
    ~SelectAction() override;

    // Appends a new checkable choice and returns it.
    QAction *addAction(const QString &text);

    int currentItem() const;
    void setCurrentItem(int index);

    QList<QAction *> actions() const;

Q_SIGNALS:
    // Emitted when the user picks a choice.
    void indexTriggered(int index);

private:
    QMenu *m_menu;
    QActionGroup *m_group;
};

#endif
