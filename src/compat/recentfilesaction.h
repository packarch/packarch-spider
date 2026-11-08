/*
 * "Open Recent" submenu backed by QSettings. Replaces KRecentFilesAction.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_RECENTFILESACTION_H
#define KPAT_COMPAT_RECENTFILESACTION_H

#include <QAction>
#include <QList>
#include <QUrl>

class QMenu;
class QSettings;

class RecentFilesAction : public QAction
{
    Q_OBJECT

public:
    explicit RecentFilesAction(const QString &text, QObject *parent = nullptr);
    ~RecentFilesAction() override;

    void addUrl(const QUrl &url);
    QList<QUrl> urls() const;

    void loadEntries(QSettings &settings);
    void saveEntries(QSettings &settings) const;

    void setMaxItems(int maxItems);

Q_SIGNALS:
    void urlSelected(const QUrl &url);

private:
    void rebuildMenu();

    QMenu *m_menu;
    QList<QUrl> m_urls;
    int m_maxItems = 10;
};

#endif
