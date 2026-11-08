/*
 *  Copyright (C) 2010 Parker Coates <coates@kde.org>
 *
 *  This program is free software; you can redistribute it and/or
 *  modify it under the terms of the GNU General Public License as
 *  published by the Free Software Foundation; either version 2 of
 *  the License, or (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 */

#include "kcardtheme.h"

// own
#include "datapaths.h"
#include "desktopfile.h"
// Qt
#include <QDir>
#include <QFileInfo>
#include <QSet>
#include <QSharedData>
#include <QStandardPaths>

class KCardThemePrivate : public QSharedData
{
public:
    KCardThemePrivate(bool isValid,
                      const QString &dirName,
                      const QString &displayName,
                      const QString &desktopFilePath,
                      const QString &graphicsFilePath,
                      const QSet<QString> &supportedFeatures,
                      const QDateTime &lastModified)
        : isValid(isValid)
        , dirName(dirName)
        , displayName(displayName)
        , desktopFilePath(desktopFilePath)
        , graphicsFilePath(graphicsFilePath)
        , supportedFeatures(supportedFeatures)
        , lastModified(lastModified){};

    const bool isValid;
    const QString dirName;
    const QString displayName;
    const QString desktopFilePath;
    const QString graphicsFilePath;
    const QSet<QString> supportedFeatures;
    const QDateTime lastModified;
};

QList<KCardTheme> KCardTheme::findAll()
{
    QList<KCardTheme> result;
    QSet<QString> seen;
    const QStringList indexFiles = DataPaths::locateAllDirs(QStringLiteral("carddecks"), true);
    for (const QString &index : indexFiles) {
        const QStringList entries = QDir(index).entryList(QDir::Dirs | QDir::NoDotAndDotDot);
        for (const QString &d : entries) {
            QString indexFilePath = index + QLatin1Char('/') + d + QLatin1String("/index.desktop");
            if (QFile::exists(indexFilePath)) {
                QString directoryName = QFileInfo(indexFilePath).dir().dirName();
                if (seen.contains(directoryName))
                    continue;
                seen.insert(directoryName);
                KCardTheme t(directoryName);
                if (t.isValid())
                    result << t;
            }
        }
    }
    return result;
}

QList<KCardTheme> KCardTheme::findAllWithFeatures(const QSet<QString> &neededFeatures)
{
    QList<KCardTheme> result;
    QSet<QString> seen;
    const QStringList indexFiles = DataPaths::locateAllDirs(QStringLiteral("carddecks"), true);
    for (const QString &index : indexFiles) {
        const QStringList entries = QDir(index).entryList(QDir::Dirs | QDir::NoDotAndDotDot);
        for (const QString &d : entries) {
            QString indexFilePath = index + QLatin1Char('/') + d + QLatin1String("/index.desktop");
            if (QFile::exists(indexFilePath)) {
                QString directoryName = QFileInfo(indexFilePath).dir().dirName();
                if (seen.contains(directoryName))
                    continue;
                seen.insert(directoryName);
                KCardTheme t(directoryName);
                if (t.isValid() && t.supportedFeatures().contains(neededFeatures))
                    result << t;
            }
        }
    }
    return result;
}

KCardTheme::KCardTheme()
    : d(nullptr)
{
}

KCardTheme::KCardTheme(const QString &dirName)
{
    bool isValid = false;
    QString displayName;
    QString desktopFilePath;
    QString graphicsFilePath;
    QStringList supportedFeatures;
    QDateTime lastModified;

    QString indexFilePath;
    if (!dirName.isEmpty()) {
        const QString relativePath = QStringLiteral("%1/index.desktop").arg(dirName);
        const QStringList deckDirs = DataPaths::locateAllDirs(QStringLiteral("carddecks"), true);
        for (const QString &deckDir : deckDirs) {
            const QString candidate = deckDir + QLatin1Char('/') + relativePath;
            if (QFile::exists(candidate)) {
                indexFilePath = candidate;
                break;
            }
        }
    }
    if (!indexFilePath.isEmpty()) {
        desktopFilePath = indexFilePath;

        const QString group = QStringLiteral("KDE Backdeck");
        DesktopFile config(indexFilePath);
        if (config.hasGroup(group)) {
            displayName = config.localizedValue(group, QStringLiteral("Name"));

            supportedFeatures =
                config.listValue(group, QStringLiteral("Features"), QStringList() << QStringLiteral("AngloAmerican") << QStringLiteral("Backs1"));

            QString svgName = config.value(group, QStringLiteral("SVG"));
            if (!svgName.isEmpty()) {
                QFileInfo indexFile(indexFilePath);
                QFileInfo svgFile(indexFile.dir(), svgName);
                // Decks copied straight from the sources ship an uncompressed
                // .svg while index.desktop references the installed .svgz.
                if (!svgFile.exists() && svgName.endsWith(QLatin1String(".svgz")))
                    svgFile = QFileInfo(indexFile.dir(), svgName.chopped(1));
                graphicsFilePath = svgFile.absoluteFilePath();

                if (svgFile.exists()) {
                    lastModified = qMax(svgFile.lastModified(), indexFile.lastModified());
                    isValid = true;
                }
            }
        }
    }

    const QSet<QString> supportedFeaturesSet(supportedFeatures.cbegin(), supportedFeatures.cend());
    d = new KCardThemePrivate(isValid, dirName, displayName, desktopFilePath, graphicsFilePath, supportedFeaturesSet, lastModified);
}

KCardTheme::KCardTheme(const KCardTheme &other)
    : d(other.d)
{
}

KCardTheme::~KCardTheme()
{
}

KCardTheme &KCardTheme::operator=(const KCardTheme &other)
{
    d = other.d;
    return *this;
}

bool KCardTheme::isValid() const
{
    return d && d->isValid;
}

QString KCardTheme::dirName() const
{
    return d ? d->dirName : QString();
}

QString KCardTheme::displayName() const
{
    return d ? d->displayName : QString();
}

QString KCardTheme::desktopFilePath() const
{
    return d ? d->desktopFilePath : QString();
}

QString KCardTheme::graphicsFilePath() const
{
    return d ? d->graphicsFilePath : QString();
}

QDateTime KCardTheme::lastModified() const
{
    return d ? d->lastModified : QDateTime();
}

QSet<QString> KCardTheme::supportedFeatures() const
{
    return d ? d->supportedFeatures : QSet<QString>();
}

bool KCardTheme::operator==(const KCardTheme &theme) const
{
    return dirName() == theme.dirName();
}

bool KCardTheme::operator!=(const KCardTheme &theme) const
{
    return !operator==(theme);
}
