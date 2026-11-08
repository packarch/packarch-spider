/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "datapaths.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QStandardPaths>

QStringList DataPaths::dataDirs()
{
    QStringList dirs;
    auto add = [&dirs](const QString &path) {
        const QString clean = QDir::cleanPath(path);
        if (!dirs.contains(clean) && QFileInfo(clean).isDir())
            dirs << clean;
    };

    add(QCoreApplication::applicationDirPath() + QStringLiteral("/../share/packarch-spider"));
#ifdef KPAT_INSTALL_DATADIR
    add(QStringLiteral(KPAT_INSTALL_DATADIR));
#endif
    const auto standard = QStandardPaths::standardLocations(QStandardPaths::GenericDataLocation);
    for (const QString &dir : standard)
        add(dir + QStringLiteral("/packarch-spider"));

    return dirs;
}

QString DataPaths::locate(const QString &relativePath)
{
    const auto dirs = dataDirs();
    for (const QString &dir : dirs) {
        const QString path = dir + QLatin1Char('/') + relativePath;
        if (QFileInfo::exists(path))
            return path;
    }
    return QString();
}

QStringList DataPaths::locateAllDirs(const QString &relativePath, bool alsoGeneric)
{
    QStringList result;
    auto add = [&result](const QString &path) {
        const QString clean = QDir::cleanPath(path);
        if (!result.contains(clean) && QFileInfo(clean).isDir())
            result << clean;
    };

    const auto dirs = dataDirs();
    for (const QString &dir : dirs)
        add(dir + QLatin1Char('/') + relativePath);

    if (alsoGeneric) {
        const auto generic = QStandardPaths::standardLocations(QStandardPaths::GenericDataLocation);
        for (const QString &dir : generic)
            add(dir + QLatin1Char('/') + relativePath);
    }
    return result;
}
