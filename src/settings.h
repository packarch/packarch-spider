/*
 * Application settings, backed by QSettings.
 *
 * This replaces the class that kconfig_compiler used to generate from
 * kpat.kcfg; the public API (static getters / setters) is unchanged.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef SETTINGS_H
#define SETTINGS_H

#include <QString>

class QSettings;

// X-macro list: (Type, getter, Setter, settings key, default value)
#define KPAT_SETTINGS_ENTRIES(X)                                                                                                                               \
    X(bool, autoDropEnabled, AutoDropEnabled, "Autodrop", true)                                                                                                \
    X(bool, solverEnabled, SolverEnabled, "Solver", true)                                                                                                      \
    X(bool, playSounds, PlaySounds, "PlaySounds", false)                                                                                                       \
    X(bool, rememberStateOnExit, RememberStateOnExit, "RememberStateOnExit", false)                                                                            \
    X(QString, cardTheme, CardTheme, "Cardname", QStringLiteral("svg-oxygen-air"))                                                                             \
    X(QString, theme, Theme, "Theme", QStringLiteral("greenblaze"))                                                                                            \
    X(bool, klondikeIsDrawOne, KlondikeIsDrawOne, "KlondikeEasy", true)                                                                                        \
    X(bool, biziliziIsExtra3, BiziliziIsExtra3, "BiziliziHard", false)                                                                                         \
    X(int, poolAnalysisEnabled, PoolAnalysisEnabled, "BiziliziPoolHints", 0)                                                                                   \
    X(int, rideHints, RideHints, "BiziliziRideHints", 0)                                                                                                       \
    X(int, spiderSuitCount, SpiderSuitCount, "SpiderSuits", 2)                                                                                                 \
    X(int, spiderStackFaceup, SpiderStackFaceup, "SpiderStackFaceup", 0)                                                                                       \
    X(int, golfSolverIterationsLimit, GolfSolverIterationsLimit, "GolfSolverIterationsLimit", 1000000)                                                         \
    X(int, freecellSolverIterationsLimit, FreecellSolverIterationsLimit, "FreecellSolverIterationsLimit", 200000)                                              \
    X(int, freecellEmptyStackFill, FreecellEmptyStackFill, "FreecellEmptyStackFill", 0)                                                                        \
    X(int, freecellSequenceBuiltBy, FreecellSequenceBuiltBy, "FreecellSequenceBuiltBy", 0)                                                                     \
    X(int, freecellReserves, FreecellReserves, "FreecellReserves", 4)                                                                                          \
    X(int, freecellStacks, FreecellStacks, "FreecellStacks", 2)                                                                                                \
    X(int, freecellDecks, FreecellDecks, "FreecellDecks", 0)                                                                                                   \
    X(int, simpleSimonSolverIterationsLimit, SimpleSimonSolverIterationsLimit, "SimpleSimonSolverIterationsLimit", 200000)                                     \
    X(int, bakersDozenSolverIterationsLimit, BakersDozenSolverIterationsLimit, "BakersDozenSolverIterationsLimit", 200000)                                     \
    X(int, bakersDozenEmptyStackFill, BakersDozenEmptyStackFill, "BakersDozenEmptyStackFill", 2)                                                               \
    X(int, bakersDozenStackFacedown, BakersDozenStackFacedown, "BakersDozenStackFacedown", 0)                                                                  \
    X(int, bakersDozenSequenceBuiltBy, BakersDozenSequenceBuiltBy, "BakersDozenSequenceBuiltBy", 2)                                                            \
    X(int, castleSolverIterationsLimit, CastleSolverIterationsLimit, "CastleSolverIterationsLimit", 200000)                                                    \
    X(int, castleEmptyStackFill, CastleEmptyStackFill, "CastleEmptyStackFill", 0)                                                                              \
    X(int, castleSequenceBuiltBy, CastleSequenceBuiltBy, "CastleSequenceBuiltBy", 2)                                                                           \
    X(int, castleReserves, CastleReserves, "CastleReserves", 0)                                                                                                \
    X(int, castleStacks, CastleStacks, "CastleStacks", 8)                                                                                                      \
    X(int, castleStackFaceup, CastleStackFaceup, "CastleStackFaceup", 1)                                                                                       \
    X(int, castleFoundation, CastleFoundation, "CastleFoundation", 1)                                                                                          \
    X(int, castleLayout, CastleLayout, "CastleLayout", 0)

class Settings
{
public:
    static Settings *self();

    // The shared settings store (~/.config/packarch-spider/packarch-spider.conf). Other parts of
    // the application (statistics, recent files, window state) use their
    // own groups in it.
    static QSettings &config();

    // Flushes the settings to disk.
    void save();

#define KPAT_SETTINGS_DECLARE(Type, getter, Setter, key, defaultValue)                                                                                         \
    static Type getter();                                                                                                                                      \
    static void set##Setter(const Type &value);                                                                                                                \
    static Type default##Setter##Value();

    KPAT_SETTINGS_ENTRIES(KPAT_SETTINGS_DECLARE)
#undef KPAT_SETTINGS_DECLARE

private:
    Settings() = default;
};

#endif
