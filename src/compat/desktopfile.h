/*
 * Tiny reader for freedesktop-style .desktop / INI files (replaces KConfig
 * for reading theme and card deck index files). Supports localized keys
 * such as Name[fr].
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_DESKTOPFILE_H
#define KPAT_COMPAT_DESKTOPFILE_H

#include <QHash>
#include <QString>
#include <QStringList>

class DesktopFile
{
public:
    explicit DesktopFile(const QString &fileName);

    bool isValid() const;
    QString fileName() const;

    bool hasGroup(const QString &group) const;

    // Raw value of @p key in @p group, or @p defaultValue.
    QString value(const QString &group, const QString &key, const QString &defaultValue = QString()) const;
    // Value of @p key in @p group, preferring the translation for the current locale.
    QString localizedValue(const QString &group, const QString &key, const QString &defaultValue = QString()) const;
    // Comma or semicolon separated list.
    QStringList listValue(const QString &group, const QString &key, const QStringList &defaultValue = QStringList()) const;

private:
    QString m_fileName;
    bool m_valid = false;
    QHash<QString, QHash<QString, QString>> m_groups;
};

#endif
