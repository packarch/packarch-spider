/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "desktopfile.h"

#include <QFile>
#include <QLocale>
#include <QTextStream>

DesktopFile::DesktopFile(const QString &fileName)
    : m_fileName(fileName)
{
    QFile file(fileName);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return;

    m_valid = true;
    QTextStream stream(&file);
    stream.setEncoding(QStringConverter::Utf8);

    QString currentGroup;
    while (!stream.atEnd()) {
        const QString line = stream.readLine().trimmed();
        if (line.isEmpty() || line.startsWith(QLatin1Char('#')) || line.startsWith(QLatin1Char(';')))
            continue;

        if (line.startsWith(QLatin1Char('[')) && line.endsWith(QLatin1Char(']'))) {
            currentGroup = line.mid(1, line.size() - 2);
            m_groups[currentGroup];
            continue;
        }

        const int eq = line.indexOf(QLatin1Char('='));
        if (eq <= 0)
            continue;
        const QString key = line.left(eq).trimmed();
        QString value = line.mid(eq + 1).trimmed();
        value.replace(QLatin1String("\\s"), QLatin1String(" "));
        value.replace(QLatin1String("\\n"), QLatin1String("\n"));
        value.replace(QLatin1String("\\t"), QLatin1String("\t"));
        value.replace(QLatin1String("\\\\"), QLatin1String("\\"));
        m_groups[currentGroup].insert(key, value);
    }
}

bool DesktopFile::isValid() const
{
    return m_valid;
}

QString DesktopFile::fileName() const
{
    return m_fileName;
}

bool DesktopFile::hasGroup(const QString &group) const
{
    return m_groups.contains(group);
}

QString DesktopFile::value(const QString &group, const QString &key, const QString &defaultValue) const
{
    const auto groupIt = m_groups.constFind(group);
    if (groupIt == m_groups.constEnd())
        return defaultValue;
    return groupIt->value(key, defaultValue);
}

QString DesktopFile::localizedValue(const QString &group, const QString &key, const QString &defaultValue) const
{
    const auto groupIt = m_groups.constFind(group);
    if (groupIt == m_groups.constEnd())
        return defaultValue;

    const QStringList languages = QLocale().uiLanguages(QLocale::TagSeparator::Underscore);
    for (const QString &lang : languages) {
        // Try full locale (fr_FR), then the language only (fr).
        QString candidate = lang;
        const auto it = groupIt->constFind(QStringLiteral("%1[%2]").arg(key, candidate));
        if (it != groupIt->constEnd() && !it->isEmpty())
            return *it;
        const int sep = candidate.indexOf(QLatin1Char('_'));
        if (sep > 0) {
            candidate = candidate.left(sep);
            const auto it2 = groupIt->constFind(QStringLiteral("%1[%2]").arg(key, candidate));
            if (it2 != groupIt->constEnd() && !it2->isEmpty())
                return *it2;
        }
    }
    return groupIt->value(key, defaultValue);
}

QStringList DesktopFile::listValue(const QString &group, const QString &key, const QStringList &defaultValue) const
{
    const QString raw = value(group, key);
    if (raw.isEmpty())
        return defaultValue;

    const QChar separator = raw.contains(QLatin1Char(';')) ? QLatin1Char(';') : QLatin1Char(',');
    QStringList result;
    const auto parts = raw.split(separator, Qt::SkipEmptyParts);
    for (const QString &part : parts)
        result << part.trimmed();
    return result;
}
