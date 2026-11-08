/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "recentfilesaction.h"

#include "i18n.h"

#include <QFileInfo>
#include <QMenu>
#include <QSettings>

RecentFilesAction::RecentFilesAction(const QString &text, QObject *parent)
    : QAction(text, parent)
    , m_menu(new QMenu())
{
    m_menu->setTitle(text);
    setMenu(m_menu);
    rebuildMenu();
}

RecentFilesAction::~RecentFilesAction()
{
    delete m_menu;
}

void RecentFilesAction::addUrl(const QUrl &url)
{
    if (url.isEmpty())
        return;
    m_urls.removeAll(url);
    m_urls.prepend(url);
    while (m_urls.size() > m_maxItems)
        m_urls.removeLast();
    rebuildMenu();
}

QList<QUrl> RecentFilesAction::urls() const
{
    return m_urls;
}

void RecentFilesAction::loadEntries(QSettings &settings)
{
    m_urls.clear();
    const QStringList entries = settings.value(QStringLiteral("RecentFiles")).toStringList();
    for (const QString &entry : entries) {
        const QUrl url = QUrl::fromUserInput(entry, QString(), QUrl::AssumeLocalFile);
        if (url.isValid() && !m_urls.contains(url))
            m_urls << url;
        if (m_urls.size() >= m_maxItems)
            break;
    }
    rebuildMenu();
}

void RecentFilesAction::saveEntries(QSettings &settings) const
{
    QStringList entries;
    for (const QUrl &url : m_urls)
        entries << (url.isLocalFile() ? url.toLocalFile() : url.toString());
    settings.setValue(QStringLiteral("RecentFiles"), entries);
}

void RecentFilesAction::setMaxItems(int maxItems)
{
    m_maxItems = qMax(1, maxItems);
    while (m_urls.size() > m_maxItems)
        m_urls.removeLast();
    rebuildMenu();
}

void RecentFilesAction::rebuildMenu()
{
    m_menu->clear();
    for (const QUrl &url : std::as_const(m_urls)) {
        const QString name = url.isLocalFile() ? QFileInfo(url.toLocalFile()).fileName() : url.fileName();
        QAction *action = m_menu->addAction(QStringLiteral("%1  [%2]").arg(name, url.toDisplayString(QUrl::PreferLocalFile)));
        connect(action, &QAction::triggered, this, [this, url]() {
            Q_EMIT urlSelected(url);
        });
    }
    if (!m_urls.isEmpty()) {
        m_menu->addSeparator();
        QAction *clear = m_menu->addAction(QIcon::fromTheme(QStringLiteral("edit-clear-history")), i18nc("@action", "Clear List"));
        connect(clear, &QAction::triggered, this, [this]() {
            m_urls.clear();
            rebuildMenu();
        });
    }
    setEnabled(!m_urls.isEmpty());
}
