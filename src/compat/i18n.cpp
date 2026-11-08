/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "i18n.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>

#include <clocale>
#include <cstring>
#include <libintl.h>

namespace
{
QByteArray s_domain = QByteArrayLiteral("packarch-spider");
// Secondary catalog with the standard strings kpat used to get from the KDE
// Frameworks (see tools/update-extra-translations.py).
QByteArray s_extraDomain = QByteArrayLiteral("packarch-spider-extra");
bool s_initialized = false;

// Returns the translation of @p key in @p domain, or nullptr if there is none.
const char *lookup(const QByteArray &domain, const char *key)
{
    const char *result = dgettext(domain.constData(), key);
    return (result == key || std::strcmp(result, key) == 0) ? nullptr : result;
}

const char *lookupPlural(const QByteArray &domain, const char *key, const char *plural, unsigned long n)
{
    const char *result = dngettext(domain.constData(), key, plural, n);
    return (std::strcmp(result, key) == 0 || std::strcmp(result, plural) == 0) ? nullptr : result;
}

QString findLocaleDir(const char *domain)
{
    const QString moName = QStringLiteral("LC_MESSAGES/%1.mo").arg(QString::fromUtf8(domain));
    QStringList candidates;
    const QString appDir = QCoreApplication::applicationDirPath();
    candidates << appDir + QStringLiteral("/../share/locale");
    candidates << appDir + QStringLiteral("/locale");
#ifdef KPAT_INSTALL_LOCALEDIR
    candidates << QStringLiteral(KPAT_INSTALL_LOCALEDIR);
#endif
    candidates << QStringLiteral("/usr/local/share/locale") << QStringLiteral("/usr/share/locale");

    for (const QString &candidate : std::as_const(candidates)) {
        QDir dir(candidate);
        if (!dir.exists())
            continue;
        const auto langs = dir.entryList(QDir::Dirs | QDir::NoDotAndDotDot);
        for (const QString &lang : langs) {
            if (QFileInfo::exists(dir.filePath(lang + QLatin1Char('/') + moName)))
                return QDir::cleanPath(dir.absolutePath());
        }
    }
    return QString();
}
}

void I18n::initialize(const char *domain)
{
    s_domain = QByteArray(domain);
    s_extraDomain = s_domain + "-extra";
    std::setlocale(LC_ALL, "");

    const QString localeDir = findLocaleDir(domain);
    for (const QByteArray &d : {s_domain, s_extraDomain}) {
        if (!localeDir.isEmpty())
            bindtextdomain(d.constData(), QFile::encodeName(localeDir).constData());
        bind_textdomain_codeset(d.constData(), "UTF-8");
    }
    s_initialized = true;
}

QString I18n::translate(const char *context, const char *text)
{
    if (!text || !*text)
        return QString();
    if (!s_initialized)
        return QString::fromUtf8(text);

    QByteArray contextKey;
    const char *key = text;
    if (context && *context) {
        contextKey = QByteArray(context) + '\004' + QByteArray(text);
        key = contextKey.constData();
    }

    const char *result = lookup(s_domain, key);
    if (!result)
        result = lookup(s_extraDomain, key);
    return QString::fromUtf8(result ? result : text);
}

QString I18n::translatePlural(const char *context, const char *singular, const char *plural, qint64 n)
{
    const unsigned long count = n < 0 ? static_cast<unsigned long>(-n) : static_cast<unsigned long>(n);
    if (!s_initialized)
        return QString::fromUtf8(count == 1 ? singular : plural);

    QByteArray contextKey;
    const char *key = singular;
    if (context && *context) {
        contextKey = QByteArray(context) + '\004' + QByteArray(singular);
        key = contextKey.constData();
    }

    const char *result = lookupPlural(s_domain, key, plural, count);
    if (!result)
        result = lookupPlural(s_extraDomain, key, plural, count);
    return QString::fromUtf8(result ? result : (count == 1 ? singular : plural));
}

QString I18n::substitute(const QString &format, const QStringList &args)
{
    if (args.isEmpty())
        return format;

    QString result;
    result.reserve(format.size() + 16 * args.size());
    const int size = format.size();
    for (int i = 0; i < size; ++i) {
        const QChar c = format.at(i);
        if (c == QLatin1Char('%') && i + 1 < size && format.at(i + 1).isDigit()) {
            int j = i + 1;
            int number = 0;
            while (j < size && format.at(j).isDigit() && j - i <= 2) {
                number = number * 10 + format.at(j).digitValue();
                ++j;
            }
            if (number >= 1 && number <= args.size()) {
                result += args.at(number - 1);
                i = j - 1;
                continue;
            }
        }
        result += c;
    }
    return result;
}

QString LocalizedString::toString() const
{
    if (isEmpty())
        return QString();

    if (m_plural) {
        QStringList args = m_args;
        return I18n::substitute(I18n::translatePlural(m_context, m_text, m_plural, m_number), args);
    }
    return I18n::substitute(I18n::translate(m_context, m_text), m_args);
}
