/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "settings.h"

#include <QSettings>
#include <QStandardPaths>

namespace
{
const QString generalGroup = QStringLiteral("General/");
}

Settings *Settings::self()
{
    static Settings instance;
    return &instance;
}

QSettings &Settings::config()
{
    static QSettings settings(QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation) + QStringLiteral("/packarch-spider/packarch-spider.conf"),
                              QSettings::IniFormat);
    return settings;
}

void Settings::save()
{
    config().sync();
}

#define KPAT_SETTINGS_DEFINE(Type, getter, Setter, key, defaultValue)                                                                                          \
    Type Settings::getter()                                                                                                                                    \
    {                                                                                                                                                          \
        return config().value(generalGroup + QStringLiteral(key), QVariant::fromValue<Type>(defaultValue)).value<Type>();                                      \
    }                                                                                                                                                          \
    void Settings::set##Setter(const Type &value)                                                                                                              \
    {                                                                                                                                                          \
        config().setValue(generalGroup + QStringLiteral(key), QVariant::fromValue<Type>(value));                                                               \
    }                                                                                                                                                          \
    Type Settings::default##Setter##Value()                                                                                                                    \
    {                                                                                                                                                          \
        return defaultValue;                                                                                                                                   \
    }

KPAT_SETTINGS_ENTRIES(KPAT_SETTINGS_DEFINE)
#undef KPAT_SETTINGS_DEFINE
