/*
 * Minimal, KDE-free replacement for KLocalizedString / ki18n.
 *
 * Translations are looked up with GNU gettext (part of glibc on Linux),
 * using the po/<lang>/packarch-spider.po catalogs compiled to .mo files.
 * The function names (i18n, i18nc, i18np, ki18n, kli18n, ...) are kept so
 * that the sources and the string extraction scripts stay unchanged.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_I18N_H
#define KPAT_COMPAT_I18N_H

#include <QLocale>
#include <QString>
#include <QStringList>

#include <type_traits>

namespace I18n
{
// Must be called once, after QCoreApplication has been constructed.
void initialize(const char *domain);

QString translate(const char *context, const char *text);
QString translatePlural(const char *context, const char *singular, const char *plural, qint64 n);
QString substitute(const QString &format, const QStringList &args);

inline QString toArg(const QString &s)
{
    return s;
}
inline QString toArg(const char *s)
{
    return QString::fromUtf8(s);
}
inline QString toArg(QChar c)
{
    return QString(c);
}
inline QString toArg(char c)
{
    return QString(QLatin1Char(c));
}
template<typename T>
inline std::enable_if_t<std::is_integral_v<T> || std::is_enum_v<T>, QString> toArg(T n)
{
    return QLocale().toString(static_cast<qlonglong>(n));
}
inline QString toArg(double d)
{
    return QLocale().toString(d);
}
}

// A translatable string whose translation is resolved lazily, when
// toString() is called. Replaces KLocalizedString and KLazyLocalizedString.
class LocalizedString
{
public:
    LocalizedString() = default;
    LocalizedString(const char *context, const char *text, const char *plural = nullptr)
        : m_context(context)
        , m_text(text)
        , m_plural(plural)
    {
    }

    QString toString() const;
    const char *untranslatedText() const
    {
        return m_text;
    }
    bool isEmpty() const
    {
        return !m_text || !*m_text;
    }

    template<typename T>
    LocalizedString subs(const T &arg) const
    {
        LocalizedString s(*this);
        if (m_plural && !s.m_hasNumber) {
            if constexpr (std::is_integral_v<T>) {
                s.m_number = arg;
                s.m_hasNumber = true;
            }
        }
        s.m_args << I18n::toArg(arg);
        return s;
    }

private:
    const char *m_context = nullptr;
    const char *m_text = nullptr;
    const char *m_plural = nullptr;
    QStringList m_args;
    qint64 m_number = 0;
    bool m_hasNumber = false;
};

template<typename... Args>
inline QString i18n(const char *text, const Args &...args)
{
    return I18n::substitute(I18n::translate(nullptr, text), {I18n::toArg(args)...});
}

template<typename... Args>
inline QString i18nc(const char *context, const char *text, const Args &...args)
{
    return I18n::substitute(I18n::translate(context, text), {I18n::toArg(args)...});
}

template<typename N, typename... Args>
inline QString i18np(const char *singular, const char *plural, N n, const Args &...args)
{
    return I18n::substitute(I18n::translatePlural(nullptr, singular, plural, n), {I18n::toArg(n), I18n::toArg(args)...});
}

template<typename N, typename... Args>
inline QString i18ncp(const char *context, const char *singular, const char *plural, N n, const Args &...args)
{
    return I18n::substitute(I18n::translatePlural(context, singular, plural, n), {I18n::toArg(n), I18n::toArg(args)...});
}

// Used by uic generated code (uic -tr tr2i18n).
inline QString tr2i18n(const char *text, const char *comment = nullptr)
{
    if (!text || !*text)
        return QString();
    return I18n::translate(comment && *comment ? comment : nullptr, text);
}

inline LocalizedString ki18n(const char *text)
{
    return LocalizedString(nullptr, text);
}
inline LocalizedString ki18nc(const char *context, const char *text)
{
    return LocalizedString(context, text);
}
inline LocalizedString ki18np(const char *singular, const char *plural)
{
    return LocalizedString(nullptr, singular, plural);
}
inline LocalizedString kli18n(const char *text)
{
    return LocalizedString(nullptr, text);
}
inline LocalizedString kli18nc(const char *context, const char *text)
{
    return LocalizedString(context, text);
}

#endif
