/*
 * Lookup of the application's data files (themes, card decks, sounds, ...).
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_DATAPATHS_H
#define KPAT_COMPAT_DATAPATHS_H

#include <QString>
#include <QStringList>

namespace DataPaths
{
// All directories that may contain kpat data, most specific first:
// <bindir>/../share/packarch-spider (build tree and install prefix), then the
// standard XDG data locations (~/.local/share/packarch-spider, /usr/share/packarch-spider, ...).
QStringList dataDirs();

// Returns the first existing file or directory named @p relativePath in dataDirs().
QString locate(const QString &relativePath);

// Returns every existing directory named @p relativePath in dataDirs()
// and, if @p alsoGeneric is set, in the generic XDG data dirs too
// (e.g. /usr/share/carddecks installed by KDE's libkdegames).
QStringList locateAllDirs(const QString &relativePath, bool alsoGeneric = false);
}

#endif
