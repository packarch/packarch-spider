/*
 * Copyright (C) 1995 Paul Olav Tvete <paul@troll.no>
 * Copyright (C) 1997 Mario Weilguni <mweilguni@sime.com>
 * Copyright (C) 2000-2009 Stephan Kulow <coolo@kde.org>
 * Copyright (C) 2009 Parker Coates <coates@kde.org>
 *
 * License of original code:
 * -------------------------------------------------------------------------
 *   Permission to use, copy, modify, and distribute this software and its
 *   documentation for any purpose and without fee is hereby granted,
 *   provided that the above copyright notice appear in all copies and that
 *   both that copyright notice and this permission notice appear in
 *   supporting documentation.
 *
 *   This file is provided AS IS with no warranties of any kind.  The author
 *   shall have no liability with respect to the infringement of copyrights,
 *   trade secrets or any patents by this file or any part thereof.  In no
 *   event will the author be liable for any lost revenue or profits or
 *   other special, indirect and consequential damages.
 * -------------------------------------------------------------------------
 *
 * License of modifications/additions made after 2009-01-01:
 * -------------------------------------------------------------------------
 *   This program is free software; you can redistribute it and/or
 *   modify it under the terms of the GNU General Public License as
 *   published by the Free Software Foundation; either version 2 of
 *   the License, or (at your option) any later version.
 *
 *   This program is distributed in the hope that it will be useful,
 *   but WITHOUT ANY WARRANTY; without even the implied warranty of
 *   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *   GNU General Public License for more details.
 *
 *   You should have received a copy of the GNU General Public License
 *   along with this program.  If not, see <http://www.gnu.org/licenses/>.
 * -------------------------------------------------------------------------
 */

#ifndef MAINWINDOW_H
#define MAINWINDOW_H

// Qt
#include <QHash>
#include <QMainWindow>
#include <QMap>
#include <QPointer>

class DealerInfo;
class DealerScene;
class GameSelectionScene;
class NumberedDealDialog;
class PatienceView;
class SoundEngine;

class KCardDeck;

class AppearanceDialog;
class RecentFilesAction;

class QAction;
class QLabel;
class QMenu;
class QToolBar;
class QUrl;

#define saved_state_file "savedstate.xml"

class MainWindow : public QMainWindow
{
    Q_OBJECT

public:
    MainWindow();
    ~MainWindow();

public Q_SLOTS:
    bool loadGame(const QUrl &url, bool addToRecentFiles);
    void slotShowGameSelectionScreen();
    void slotGameSelected(int id);

protected Q_SLOTS:
    void newGame();
    void startRandom();
    void loadGame();
    void loadGame(const QUrl &url);
    void restart();
    void newNumberedDeal();
    void startNumbered(int gameId, int dealNumber);
    void nextDeal();
    void previousDeal();
    void saveGame();
    void showStats();

    void undoMove();
    void redoMove();

    void toggleDrop();
    void toggleHints();
    void toggleDemo();
    void toggleDemoAction(bool active);
    void toggleMenubar();
    void toggleFullscreen(bool fullScreen);

    void setAutoDropEnabled(bool enabled);
    void enableSolver(bool enable);
    void enableSounds(bool enable);
    void enableRememberState(bool enable);
    void slotPickRandom();
    void configureAppearance();
    void appearanceChanged();

    void helpGame();
    void helpContents();
    void showAbout();

    void updateSolverDescription(const QString &text);
    void slotUpdateMoves(int moves);

protected:
    void closeEvent(QCloseEvent *e) override;

private Q_SLOTS:
    void slotSnapshot();
    void slotSnapshot2();
    void generateThemePreview();

private:
    void setupActions();
    void setupMenus();
    QAction *addAction(const QString &name, const QString &text, const QString &iconName = QString(), const QKeySequence &shortcut = QKeySequence());
    QAction *addToggleAction(const QString &name, const QString &text, const QString &iconName = QString(), const QKeySequence &shortcut = QKeySequence());
    QAction *action(const QString &name) const;
    void setCaption(const QString &caption);
    void setGameType(int id);
    void setGameCaption();
    void startNew(int gameNumber);
    void updateActions();
    void updateGameActionList();
    void updateSoundEngine();

    // Members
    QAction *m_leftAction;
    QAction *m_rightAction;
    QAction *m_upAction;
    QAction *m_downAction;
    QAction *m_cancelAction;
    QAction *m_pickUpSetDownAction;

    QHash<QString, QAction *> m_actions;

    QMenu *m_moveMenu;
    QMenu *m_settingsMenu;
    QAction *m_settingsOptionsSeparator;
    QToolBar *m_toolBar;
    QList<QAction *> m_pluggedGameActions;
    // The per-game options belong to the DealerScene and are destroyed with
    // it, possibly before we unplug them: QPointer turns them into nullptr.
    QList<QPointer<QAction>> m_pluggedDealerOptions;
    AppearanceDialog *m_appearanceDialog;

    RecentFilesAction *m_recentFilesAction;
    QAction *m_saveAction;
    QAction *m_undoAction;
    QAction *m_redoAction;
    QAction *m_demoAction;
    QAction *m_hintAction;
    QAction *m_drawAction;
    QAction *m_dealAction;
    QAction *m_redealAction;
    QAction *m_dropAction;
    QAction *m_autoDropEnabledAction;
    QAction *m_solverEnabledAction;
    QAction *m_rememberStateAction;
    QAction *m_playSoundsAction;
    QAction *m_showMenubarAction;
    QAction *m_fullScreenAction;
    QAction *m_gameHelpAction;

    QMap<int, const DealerInfo *> m_dealer_map;
    QMap<int, const DealerInfo *>::const_iterator m_dealer_it;

    PatienceView *m_view;
    DealerScene *m_dealer;
    GameSelectionScene *m_selector;
    KCardDeck *m_cardDeck;
    SoundEngine *m_soundEngine;

    NumberedDealDialog *m_dealDialog;

    QLabel *m_solverStatusLabel;
    QLabel *m_moveCountStatusLabel;
};

#endif
