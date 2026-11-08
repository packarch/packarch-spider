/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "selectaction.h"

#include <QActionGroup>
#include <QMenu>

SelectAction::SelectAction(const QString &text, QObject *parent)
    : QAction(text, parent)
    , m_menu(new QMenu())
    , m_group(new QActionGroup(this))
{
    m_group->setExclusive(true);
    m_menu->setTitle(text);
    setMenu(m_menu);
    connect(this, &QAction::changed, this, [this]() {
        m_menu->setTitle(this->text());
    });
    connect(m_group, &QActionGroup::triggered, this, [this](QAction *action) {
        const int index = m_group->actions().indexOf(action);
        if (index >= 0)
            Q_EMIT indexTriggered(index);
    });
}

SelectAction::~SelectAction()
{
    delete m_menu;
}

QAction *SelectAction::addAction(const QString &text)
{
    QAction *action = new QAction(text, m_group);
    action->setCheckable(true);
    m_menu->addAction(action);
    return action;
}

int SelectAction::currentItem() const
{
    QAction *checked = m_group->checkedAction();
    return checked ? m_group->actions().indexOf(checked) : -1;
}

void SelectAction::setCurrentItem(int index)
{
    const auto actions = m_group->actions();
    if (index >= 0 && index < actions.size()) {
        actions.at(index)->setChecked(true);
    } else if (QAction *checked = m_group->checkedAction()) {
        m_group->setExclusive(false);
        checked->setChecked(false);
        m_group->setExclusive(true);
    }
}

QList<QAction *> SelectAction::actions() const
{
    return m_group->actions();
}
