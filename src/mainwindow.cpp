/*
 * Copyright (C) 1995 Paul Olav Tvete <paul@troll.no>
 * Copyright (C) 1997 Mario Weilguni <mweilguni@sime.com>
 * Copyright (C) 2000-2009 Stephan Kulow <coolo@kde.org>
 * Copyright (C) 2009-2010 Parker Coates <coates@kde.org>
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

#include "mainwindow.h"

// own
#include "appearancedialog.h"
#include "confirmdialog.h"
#include "dealer.h"
#include "dealerinfo.h"
#include "gameselectionscene.h"
#include "i18n.h"
#include "kpat_debug.h"
#include "kpat_version.h"
#include "numbereddealdialog.h"
#include "recentfilesaction.h"
#include "renderer.h"
#include "settings.h"
#include "soundengine.h"
#include "statisticsdialog.h"
#include "view.h"
// KCardGame
#include "kcardtheme.h"
// Qt
#include <QAction>
#include <QApplication>
#include <QBuffer>
#include <QDesktopServices>
#include <QDir>
#include <QFile>
#include <QFileDialog>
#include <QIcon>
#include <QKeySequence>
#include <QLabel>
#include <QList>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPainter>
#include <QPointer>
#include <QRandomGenerator>
#include <QScreen>
#include <QSettings>
#include <QStandardPaths>
#include <QStatusBar>
#include <QTimer>
#include <QToolBar>
#include <QUrl>
#include <QXmlStreamReader>

namespace
{
// Saved games are XML files. File name filters are used instead of MIME type
// filters so that the dialogs also work when the game runs from the build
// tree, before mimetypes/packarch-spider.xml is installed.
QString saveFileFilter()
{
    return i18nc("@item:inlistbox file type", "%1 saved game", QStringLiteral(KPAT_APP_NAME)) + QStringLiteral(" (*.pspider)");
}
QString legacySaveFileFilter()
{
    return i18nc("@item:inlistbox file type", "%1 saved game, legacy format", QStringLiteral(KPAT_APP_NAME)) + QStringLiteral(" (*.pspider-legacy)");
}
QString oldKPatienceFileFilter()
{
    return i18nc("@item:inlistbox file type", "KPatience saved game") + QStringLiteral(" (*.kpat *.kpatience)");
}
QString allFilesFilter()
{
    return i18nc("@item:inlistbox file type", "All files") + QStringLiteral(" (*)");
}
const QString windowGroup(QStringLiteral("MainWindow/"));
const QString handbookUrl(QStringLiteral("https://docs.kde.org/stable_kf6/en/kpat/kpat/"));

QString lastDirectory()
{
    const QString dir = Settings::config().value(windowGroup + QStringLiteral("LastDirectory")).toString();
    return dir.isEmpty() ? QDir::homePath() : dir;
}

void setLastDirectory(const QString &filePath)
{
    Settings::config().setValue(windowGroup + QStringLiteral("LastDirectory"), QFileInfo(filePath).absolutePath());
}
}

MainWindow::MainWindow()
    : QMainWindow(nullptr)
    , m_moveMenu(nullptr)
    , m_settingsMenu(nullptr)
    , m_settingsOptionsSeparator(nullptr)
    , m_toolBar(nullptr)
    , m_appearanceDialog(nullptr)
    , m_view(nullptr)
    , m_dealer(nullptr)
    , m_selector(nullptr)
    , m_cardDeck(nullptr)
    , m_soundEngine(nullptr)
    , m_dealDialog(nullptr)
{
    setObjectName(QStringLiteral("MainWindow"));

    setupActions();

    const auto games = DealerInfoList::self()->games();
    for (const DealerInfo *di : games) {
        m_dealer_map.insert(di->baseId(), di);
        const auto subtypeIds = di->subtypeIds();
        for (int id : subtypeIds)
            m_dealer_map.insert(id, di);
    }
    m_dealer_it = m_dealer_map.constEnd();

    m_view = new PatienceView(this);
    setCentralWidget(m_view);

    setupMenus();

    m_solverStatusLabel = new QLabel(QString(), statusBar());
    m_solverStatusLabel->setAlignment(Qt::AlignLeft | Qt::AlignVCenter);
    statusBar()->addWidget(m_solverStatusLabel, 1);

    m_moveCountStatusLabel = new QLabel(QString(), statusBar());
    m_moveCountStatusLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    statusBar()->addWidget(m_moveCountStatusLabel, 0);

    // Restore the window size, position, toolbar and statusbar state.
    QSettings &config = Settings::config();
    const QByteArray geometry = config.value(windowGroup + QStringLiteral("Geometry")).toByteArray();
    if (geometry.isEmpty() || !restoreGeometry(geometry)) {
        const QRect screenSize = screen()->availableGeometry();
        resize(screenSize.size() * 0.7);
    }
    restoreState(config.value(windowGroup + QStringLiteral("State")).toByteArray());
    menuBar()->setVisible(config.value(windowGroup + QStringLiteral("MenuBarVisible"), true).toBool());
    statusBar()->setVisible(config.value(windowGroup + QStringLiteral("StatusBarVisible"), true).toBool());

    m_showMenubarAction->setChecked(!menuBar()->isHidden());
    action(QStringLiteral("options_show_statusbar"))->setChecked(!statusBar()->isHidden());
}

MainWindow::~MainWindow()
{
    m_recentFilesAction->saveEntries(Settings::config());

    Settings::self()->save();

    delete m_dealer;
    delete m_view;
    Renderer::deleteSelf();
}

QAction *MainWindow::addAction(const QString &name, const QString &text, const QString &iconName, const QKeySequence &shortcut)
{
    auto a = new QAction(text, this);
    a->setObjectName(name);
    if (!iconName.isEmpty())
        a->setIcon(QIcon::fromTheme(iconName));
    if (!shortcut.isEmpty())
        a->setShortcut(shortcut);
    // Make shortcuts work even when the menubar or toolbar are hidden.
    QWidget::addAction(a);
    m_actions.insert(name, a);
    return a;
}

QAction *MainWindow::addToggleAction(const QString &name, const QString &text, const QString &iconName, const QKeySequence &shortcut)
{
    QAction *a = addAction(name, text, iconName, shortcut);
    a->setCheckable(true);
    return a;
}

QAction *MainWindow::action(const QString &name) const
{
    QAction *a = m_actions.value(name);
    Q_ASSERT_X(a, "MainWindow::action", qPrintable(name));
    return a;
}

void MainWindow::setCaption(const QString &caption)
{
    setWindowTitle(caption);
}

void MainWindow::setupActions()
{
    QAction *a;

    // Game Menu
    a = addAction(QStringLiteral("new_game"),
                  i18nc("@action Start a new game of a different type", "New &Game…"),
                  QStringLiteral("document-new"),
                  QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_N));
    connect(a, &QAction::triggered, this, &MainWindow::slotShowGameSelectionScreen);

    a = addAction(QStringLiteral("new_deal"),
                  i18nc("@action Start a new game of without changing the game type", "New &Deal"),
                  QStringLiteral("shuffle"),
                  QKeySequence(Qt::CTRL | Qt::Key_N));
    connect(a, &QAction::triggered, this, &MainWindow::newGame);

    a = addAction(QStringLiteral("new_numbered_deal"),
                  i18nc("@action Start a game by giving its particular number", "New &Numbered Deal…"),
                  QString(),
                  QKeySequence(Qt::CTRL | Qt::Key_D));
    connect(a, &QAction::triggered, this, &MainWindow::newNumberedDeal);

    a = addAction(QStringLiteral("game_restart"),
                  i18nc("@action Replay the current deal from the start", "Restart Deal"),
                  QStringLiteral("view-refresh"),
                  QKeySequence(Qt::Key_F5));
    connect(a, &QAction::triggered, this, &MainWindow::restart);

    // Note that this action is not shown in the menu or toolbar. It is
    // only provided for advanced users who can use it by shorcut.
    a = addAction(QStringLiteral("next_deal"),
                  i18nc("@action Start the game with the number one greater than the current one", "Next Deal"),
                  QStringLiteral("go-next"),
                  QKeySequence(Qt::CTRL | Qt::Key_Plus));
    connect(a, &QAction::triggered, this, &MainWindow::nextDeal);

    // Note that this action is not shown in the menu or toolbar. It is
    // only provided for advanced users who can use it by shorcut.
    a = addAction(QStringLiteral("previous_deal"),
                  i18nc("@action Start the game with the number one less than the current one", "Previous Deal"),
                  QStringLiteral("go-previous"),
                  QKeySequence(Qt::CTRL | Qt::Key_Minus));
    connect(a, &QAction::triggered, this, &MainWindow::previousDeal);

    a = addAction(QStringLiteral("game_load"), i18nc("@action", "&Open…"), QStringLiteral("document-open"), QKeySequence::Open);
    a->setToolTip(i18nc("@info:tooltip", "Open a saved game"));
    connect(a, &QAction::triggered, this, qOverload<>(&MainWindow::loadGame));

    m_recentFilesAction = new RecentFilesAction(i18nc("@action", "Open &Recent"), this);
    m_recentFilesAction->setIcon(QIcon::fromTheme(QStringLiteral("document-open-recent")));
    m_recentFilesAction->setObjectName(QStringLiteral("game_load_recent"));
    m_actions.insert(QStringLiteral("game_load_recent"), m_recentFilesAction);
    connect(m_recentFilesAction, &RecentFilesAction::urlSelected, this, qOverload<const QUrl &>(&MainWindow::loadGame));
    m_recentFilesAction->loadEntries(Settings::config());

    m_saveAction = addAction(QStringLiteral("game_save_as"),
                             i18nc("@action", "Save &As…"),
                             QStringLiteral("document-save-as"),
                             QKeySequence(Qt::CTRL | Qt::Key_S));
    m_saveAction->setToolTip(i18nc("@info:tooltip", "Save the current game"));
    connect(m_saveAction, &QAction::triggered, this, &MainWindow::saveGame);

    a = addAction(QStringLiteral("game_stats"), i18nc("@action", "Statistics"), QStringLiteral("games-highscores"));
    connect(a, &QAction::triggered, this, &MainWindow::showStats);

    a = addAction(QStringLiteral("game_quit"), i18nc("@action", "&Quit"), QStringLiteral("application-exit"), QKeySequence(Qt::CTRL | Qt::Key_Q));
    a->setMenuRole(QAction::QuitRole);
    connect(a, &QAction::triggered, this, &MainWindow::close);

    // Move Menu
    m_undoAction = addAction(QStringLiteral("move_undo"), i18nc("@action", "Und&o"), QStringLiteral("edit-undo"), QKeySequence::Undo);
    m_undoAction->setToolTip(i18nc("@info:tooltip", "Undo the last move"));
    connect(m_undoAction, &QAction::triggered, this, &MainWindow::undoMove);

    m_redoAction = addAction(QStringLiteral("move_redo"), i18nc("@action", "Re&do"), QStringLiteral("edit-redo"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_Z));
    m_redoAction->setToolTip(i18nc("@info:tooltip", "Redo the latest move"));
    connect(m_redoAction, &QAction::triggered, this, &MainWindow::redoMove);

    m_demoAction = addToggleAction(QStringLiteral("move_demo"), i18nc("@action", "&Demo"), QStringLiteral("media-playback-start"), QKeySequence(Qt::Key_D));
    m_demoAction->setToolTip(i18nc("@info:tooltip", "Play a demo"));
    connect(m_demoAction, &QAction::triggered, this, &MainWindow::toggleDemo);

    m_hintAction = addToggleAction(QStringLiteral("move_hint"), i18nc("@action", "&Hint"), QStringLiteral("games-hint"), QKeySequence(Qt::Key_H));
    m_hintAction->setToolTip(i18nc("@info:tooltip", "Give a hint"));
    connect(m_hintAction, &QAction::triggered, this, &MainWindow::toggleHints);

    m_drawAction = addAction(QStringLiteral("move_draw"),
                             i18nc("@action Take one or more cards from the deck, flip them, and place them in play", "Dra&w"),
                             QStringLiteral(KPAT_APP_ICON),
                             QKeySequence(Qt::Key_Tab));

    m_dealAction = addAction(QStringLiteral("move_deal"),
                             i18nc("@action Deal a new row of cards from the deck", "Dea&l Row"),
                             QStringLiteral(KPAT_APP_ICON),
                             QKeySequence(Qt::Key_Return));

    m_redealAction = addAction(QStringLiteral("move_redeal"),
                               i18nc("@action Collect the cards in play, shuffle them and redeal them", "&Redeal"),
                               QStringLiteral("roll"),
                               QKeySequence(Qt::Key_R));

    m_dropAction = addToggleAction(QStringLiteral("move_drop"),
                                   i18nc("@action Automatically move cards to the foundation piles", "Dro&p"),
                                   QStringLiteral("games-endturn"),
                                   QKeySequence(Qt::Key_P));
    connect(m_dropAction, &QAction::triggered, this, &MainWindow::toggleDrop);

    // Settings Menu
    m_autoDropEnabledAction = addToggleAction(QStringLiteral("enable_autodrop"), i18nc("@option:check", "&Enable Autodrop"));
    connect(m_autoDropEnabledAction, &QAction::triggered, this, &MainWindow::setAutoDropEnabled);
    m_autoDropEnabledAction->setChecked(Settings::autoDropEnabled());

    m_solverEnabledAction = addToggleAction(QStringLiteral("enable_solver"), i18nc("@option:check", "E&nable Solver"));
    connect(m_solverEnabledAction, &QAction::triggered, this, &MainWindow::enableSolver);
    m_solverEnabledAction->setChecked(Settings::solverEnabled());

    m_playSoundsAction = addToggleAction(QStringLiteral("play_sounds"), i18nc("@option:check", "Play Sounds"), QStringLiteral("speaker"));
    connect(m_playSoundsAction, &QAction::triggered, this, &MainWindow::enableSounds);
    m_playSoundsAction->setChecked(Settings::playSounds());

    m_rememberStateAction = addToggleAction(QStringLiteral("remember_state"), i18nc("@option:check", "&Remember State on Exit"));
    connect(m_rememberStateAction, &QAction::triggered, this, &MainWindow::enableRememberState);
    m_rememberStateAction->setChecked(Settings::rememberStateOnExit());

    m_showMenubarAction = addToggleAction(QStringLiteral("options_show_menubar"),
                                          i18nc("@action", "Show &Menubar"),
                                          QStringLiteral("show-menu"),
                                          QKeySequence(Qt::CTRL | Qt::Key_M));
    connect(m_showMenubarAction, &QAction::triggered, this, &MainWindow::toggleMenubar);

    a = addToggleAction(QStringLiteral("options_show_statusbar"), i18nc("@action", "Show St&atusbar"));
    connect(a, &QAction::toggled, statusBar(), &QWidget::setVisible);

    m_fullScreenAction = addToggleAction(QStringLiteral("fullscreen"),
                                         i18nc("@action", "F&ull Screen Mode"),
                                         QStringLiteral("view-fullscreen"),
                                         QKeySequence::FullScreen);
    connect(m_fullScreenAction, &QAction::toggled, this, &MainWindow::toggleFullscreen);

    a = addAction(QStringLiteral("options_configure"),
                  i18nc("@action", "&Configure %1…", QStringLiteral(KPAT_APP_NAME)),
                  QStringLiteral("configure"),
                  QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_Comma));
    a->setMenuRole(QAction::PreferencesRole);
    connect(a, &QAction::triggered, this, &MainWindow::configureAppearance);

    // Help Menu
    a = addAction(QStringLiteral("help_contents"), i18nc("@action", "%1 &Handbook", QStringLiteral(KPAT_APP_NAME)), QStringLiteral("help-contents"), QKeySequence::HelpContents);
    connect(a, &QAction::triggered, this, &MainWindow::helpContents);

    m_gameHelpAction = addAction(QStringLiteral("help_game"), QString(), QStringLiteral("help-contents"), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_F1));
    connect(m_gameHelpAction, &QAction::triggered, this, &MainWindow::helpGame);

    a = addAction(QStringLiteral("help_about_app"), i18nc("@action", "&About %1", QStringLiteral(KPAT_APP_NAME)), QStringLiteral(KPAT_APP_ICON));
    a->setMenuRole(QAction::AboutRole);
    connect(a, &QAction::triggered, this, &MainWindow::showAbout);

    a = addAction(QStringLiteral("help_about_qt"), i18nc("@action", "About &Qt"), QStringLiteral("qtlogo"));
    a->setMenuRole(QAction::AboutQtRole);
    connect(a, &QAction::triggered, qApp, &QApplication::aboutQt);

    // Hidden actions
    if (!qEnvironmentVariableIsEmpty("KDE_DEBUG") || !qEnvironmentVariableIsEmpty("KPAT_DEBUG")) // developer shortcut
    {
        a = addAction(QStringLiteral("themePreview"), i18nc("@action", "Generate a Theme Preview Image"), QString(), QKeySequence(Qt::Key_F7));
        connect(a, &QAction::triggered, this, &MainWindow::generateThemePreview);

        a = addAction(QStringLiteral("snapshot"), i18nc("@action", "Take Game Preview Snapshots"), QString(), QKeySequence(Qt::Key_F8));
        connect(a, &QAction::triggered, this, &MainWindow::slotSnapshot);

        a = addAction(QStringLiteral("random_set"), i18nc("@action", "Random Cards"), QString(), QKeySequence(Qt::Key_F9));
        connect(a, &QAction::triggered, this, &MainWindow::slotPickRandom);
    }

    // Keyboard navigation actions
    m_leftAction = addAction(QStringLiteral("focus_left"), i18nc("@action", "Move Focus to Previous Pile"), QString(), QKeySequence(Qt::Key_Left));
    m_rightAction = addAction(QStringLiteral("focus_right"), i18nc("@action", "Move Focus to Next Pile"), QString(), QKeySequence(Qt::Key_Right));
    m_upAction = addAction(QStringLiteral("focus_up"), i18nc("@action", "Move Focus to Card Below"), QString(), QKeySequence(Qt::Key_Up));
    m_downAction = addAction(QStringLiteral("focus_down"), i18nc("@action", "Move Focus to Card Above"), QString(), QKeySequence(Qt::Key_Down));
    m_cancelAction = addAction(QStringLiteral("focus_cancel"), i18nc("@action", "Cancel Focus"), QString(), QKeySequence(Qt::Key_Escape));
    m_pickUpSetDownAction = addAction(QStringLiteral("focus_activate"), i18nc("@action", "Pick Up or Set Down Focus"), QString(), QKeySequence(Qt::Key_Space));
}

void MainWindow::setupMenus()
{
    // Equivalent of the former kpatui.rc
    QMenu *gameMenu = menuBar()->addMenu(i18nc("@title:menu", "&Game"));
    gameMenu->addAction(action(QStringLiteral("new_game")));
    gameMenu->addSeparator();
    gameMenu->addAction(action(QStringLiteral("new_deal")));
    gameMenu->addAction(action(QStringLiteral("new_numbered_deal")));
    gameMenu->addAction(action(QStringLiteral("game_restart")));
    gameMenu->addSeparator();
    gameMenu->addAction(action(QStringLiteral("game_load")));
    gameMenu->addAction(m_recentFilesAction);
    gameMenu->addAction(action(QStringLiteral("game_save_as")));
    gameMenu->addSeparator();
    gameMenu->addAction(action(QStringLiteral("game_stats")));
    gameMenu->addSeparator();
    gameMenu->addAction(action(QStringLiteral("game_quit")));

    m_moveMenu = menuBar()->addMenu(i18n("Move"));
    m_moveMenu->addAction(m_undoAction);
    m_moveMenu->addAction(m_redoAction);
    m_moveMenu->addSeparator();
    // "game_actions" are appended here dynamically.

    m_settingsMenu = menuBar()->addMenu(i18nc("@title:menu", "&Settings"));
    // "dealer_options" are inserted here dynamically.
    m_settingsOptionsSeparator = m_settingsMenu->addSeparator();
    m_settingsMenu->addAction(m_autoDropEnabledAction);
    m_settingsMenu->addAction(m_solverEnabledAction);
    m_settingsMenu->addAction(m_playSoundsAction);
    m_settingsMenu->addAction(m_rememberStateAction);
    m_settingsMenu->addSeparator();
    m_settingsMenu->addAction(m_showMenubarAction);

    m_toolBar = addToolBar(i18n("Main Toolbar"));
    m_toolBar->setObjectName(QStringLiteral("mainToolBar"));
    m_toolBar->setToolButtonStyle(Qt::ToolButtonFollowStyle);
    m_toolBar->addAction(action(QStringLiteral("new_game")));
    m_toolBar->addAction(action(QStringLiteral("new_deal")));
    m_toolBar->addSeparator();
    m_toolBar->addAction(m_undoAction);
    m_toolBar->addAction(m_redoAction);
    m_toolBar->addSeparator();
    // "game_actions" are appended here dynamically.

    QAction *toolBarAction = m_toolBar->toggleViewAction();
    toolBarAction->setText(i18nc("@action", "Show &Toolbar"));
    m_settingsMenu->addAction(toolBarAction);
    m_settingsMenu->addAction(action(QStringLiteral("options_show_statusbar")));
    m_settingsMenu->addAction(m_fullScreenAction);
    m_settingsMenu->addSeparator();
    m_settingsMenu->addAction(action(QStringLiteral("options_configure")));

    QMenu *helpMenu = menuBar()->addMenu(i18nc("@title:menu", "&Help"));
    helpMenu->addAction(action(QStringLiteral("help_contents")));
    helpMenu->addAction(m_gameHelpAction);
    helpMenu->addSeparator();
    helpMenu->addAction(action(QStringLiteral("help_about_app")));
    helpMenu->addAction(action(QStringLiteral("help_about_qt")));
}

void MainWindow::undoMove()
{
    if (m_dealer)
        m_dealer->undo();
}

void MainWindow::redoMove()
{
    if (m_dealer)
        m_dealer->redo();
}

void MainWindow::helpGame()
{
    if (m_dealer && m_dealer_map.contains(m_dealer->gameId())) {
        const DealerInfo *di = m_dealer_map.value(m_dealer->gameId());
        QString anchor = di->untranslatedBaseName();
        anchor = anchor.toLower();
        anchor = anchor.remove(QLatin1Char('\'')).replace(QLatin1Char('&'), QLatin1String("and")).replace(QLatin1Char(' '), QLatin1Char('-'));
        QDesktopServices::openUrl(QUrl(handbookUrl + QLatin1String("rules-specific.html#") + anchor));
    }
}

void MainWindow::helpContents()
{
    QDesktopServices::openUrl(QUrl(handbookUrl));
}

void MainWindow::showAbout()
{
    struct Author {
        const char *name;
        const char *task;
        const char *email;
    };
    static const Author authors[] = {
        {"Paul Olav Tvete", "Author of original Qt version", "paul@troll.no"},
        {"Mario Weilguni", "Initial KDE port", "mweilguni@kde.org"},
        {"Matthias Ettrich", nullptr, "ettrich@kde.org"},
        {"Rodolfo Borges", "New game types", "barrett@9hells.org"},
        {"Peter H. Ruegg", nullptr, "kpat@incense.org"},
        {"Michael Koch", "Bug fixes", "koch@kde.org"},
        {"Marcus Meissner", "Shuffle algorithm for game numbers", "mm@caldera.de"},
        {"Tom Holroyd", "Initial patience solver", "tomh@kurage.nimh.nih.gov"},
        {"Stephan Kulow", "Rewrite and current maintainer", "coolo@kde.org"},
        {"Erik Sigra", "Klondike improvements", "sigra@home.se"},
        {"Josh Metzler", "Spider implementation", "joshdeb@metzlers.org"},
        {"Maren Pakura", "Documentation", "maren@kde.org"},
        {"Inge Wallin", "Bug fixes", "inge@lysator.liu.se"},
        {"Simon Hürlimann", "Menu and toolbar work", "simon.huerlimann@huerlisi.ch"},
        {"Parker Coates", "Cleanup and polish", "coates@kde.org"},
        {"Shlomi Fish", "Integration with Freecell Solver and further work", "shlomif@cpan.org"},
        {"Michael Lang", "New game types", "criticaltemp@protonmail.com"},
    };

    QString authorList;
    for (const Author &author : authors) {
        authorList += QStringLiteral("<li><b>%1</b>").arg(i18n(author.name).toHtmlEscaped());
        if (author.task)
            authorList += QStringLiteral(" — %1").arg(i18n(author.task).toHtmlEscaped());
        authorList += QStringLiteral(" &lt;<a href=\"mailto:%1\">%1</a>&gt;</li>").arg(QString::fromLatin1(author.email));
    }

    const QString text = QStringLiteral(
                             "<h2>%1 %2</h2>"
                             "<p>%3</p>"
                             "<p>%8</p>"
                             "<p>%4</p>"
                             "<p>%5</p>"
                             "<h3>%6</h3><ul>%7</ul>")
                             .arg(QStringLiteral(KPAT_APP_NAME),
                                  QStringLiteral(KPAT_VERSION_STRING),
                                  i18n("Patience card games").toHtmlEscaped(),
                                  i18n("© 1995 Paul Olav Tvete\n© 2000 Stephan Kulow").toHtmlEscaped().replace(QLatin1Char('\n'), QLatin1String("<br/>")),
                                  i18nc("@info", "License: %1", i18nc("@item license", "GNU General Public License Version 2")),
                                  i18nc("@title", "Authors"),
                                  authorList,
                                  i18n("Based on %1, the patience game of the KDE project.",
                                       QStringLiteral("<a href=\"https://apps.kde.org/kpat\">KPatience</a>")));

    QMessageBox box(this);
    box.setWindowTitle(i18nc("@title:window", "About %1", QStringLiteral(KPAT_APP_NAME)));
    box.setIconPixmap(windowIcon().pixmap(64, 64));
    box.setTextFormat(Qt::RichText);
    box.setText(text);
    box.setStandardButtons(QMessageBox::Close);
    box.exec();
}

void MainWindow::setAutoDropEnabled(bool enabled)
{
    Settings::setAutoDropEnabled(enabled);
    if (m_dealer) {
        m_dealer->setAutoDropEnabled(enabled);
        if (enabled)
            m_dealer->startDrop();
        else
            m_dealer->stopDrop();
    }
    updateGameActionList();
}

void MainWindow::enableSolver(bool enable)
{
    Settings::setSolverEnabled(enable);
    m_solverStatusLabel->setText(QString());
    if (m_dealer) {
        m_dealer->setSolverEnabled(enable);
        if (enable)
            m_dealer->startSolver();
    }
}

void MainWindow::enableSounds(bool enable)
{
    Settings::setPlaySounds(enable);
    updateSoundEngine();
}

void MainWindow::enableRememberState(bool enable)
{
    Settings::setRememberStateOnExit(enable);
}

void MainWindow::newGame()
{
    if (m_dealer && m_dealer->allowedToStartNewGame())
        startRandom();
}

void MainWindow::restart()
{
    startNew(-1);
}

void MainWindow::startRandom()
{
    const int gameNumber = int(QRandomGenerator::global()->bounded(quint32(1), quint32(INT_MAX)));
    startNew(gameNumber);
}

void MainWindow::startNew(int gameNumber)
{
    m_dealer->startNew(gameNumber);
    setGameCaption();
}

void MainWindow::slotPickRandom()
{
    QList<KCardTheme> themes = KCardTheme::findAll();
    KCardTheme theme = themes.at(QRandomGenerator::global()->bounded(themes.size()));
    Settings::setCardTheme(theme.dirName());

    appearanceChanged();
}

void MainWindow::configureAppearance()
{
    if (!m_appearanceDialog) {
        m_appearanceDialog = new AppearanceDialog(this);
        connect(m_appearanceDialog, &AppearanceDialog::settingsChanged, this, &MainWindow::appearanceChanged);
    }
    m_appearanceDialog->show();
    m_appearanceDialog->raise();
    m_appearanceDialog->activateWindow();
}

void MainWindow::appearanceChanged()
{
    if (m_cardDeck && Settings::cardTheme() != m_cardDeck->theme().dirName()) {
        KCardTheme theme(Settings::cardTheme());
        if (theme.isValid()) {
            m_cardDeck->setTheme(KCardTheme(theme));
            if (m_dealer)
                m_dealer->relayoutScene();
        }
    }
    // The game theme may have changed too: repaint piles and the selection screen.
    if (m_view && m_view->scene())
        m_view->scene()->update();
}

void MainWindow::setGameCaption()
{
    QString caption;
    if (m_dealer) {
        const DealerInfo *di = m_dealer_map.value(m_dealer->gameId());
        caption = QStringLiteral("%1 - %2").arg(di->baseName()).arg(m_dealer->gameNumber());
    }
    setCaption(caption);
}

void MainWindow::slotGameSelected(int id)
{
    if (m_dealer_map.contains(id)) {
        setGameType(id);
        QTimer::singleShot(0, this, &MainWindow::startRandom);
    }
}

void MainWindow::setGameType(int id)
{
    // Only bother calling creating a new DealerScene if we don't already have
    // the right DealerScene open.
    if (m_dealer && m_dealer_map.value(id) == m_dealer_map.value(m_dealer->gameId())) {
        m_dealer->recordGameStatistics();
        m_dealer->mapOldId(id);
        return;
    }

    // If we're replacing an existing DealerScene, record the stats of the
    // game already in progress.
    if (m_dealer) {
        m_dealer->recordGameStatistics();
        delete m_dealer;
        m_view->setScene(nullptr);
        m_dealer = nullptr;
    }

    if (!m_cardDeck) {
        KCardTheme theme = KCardTheme(Settings::cardTheme());
        if (!theme.isValid())
            theme = KCardTheme(Settings::defaultCardThemeValue());

        m_cardDeck = new KCardDeck(theme, this);
    }

    const DealerInfo *di = m_dealer_map.value(id, DealerInfoList::self()->games().first());
    m_dealer = di->createGame();
    m_dealer->setDeck(m_cardDeck);
    m_dealer->initialize();
    m_dealer->mapOldId(id);
    m_dealer->setSolverEnabled(m_solverEnabledAction->isChecked());
    m_dealer->setAutoDropEnabled(m_autoDropEnabledAction->isChecked());

    m_view->setScene(m_dealer);

    m_gameHelpAction->setText(
        i18nc("@action Is disabled and changes to \"Help &with Current Game\" when"
              " there is no current game.",
              "Help &with %1",
              di->baseName().replace(QLatin1Char('&'), QLatin1String("&&"))));

    connect(m_dealer, &DealerScene::solverStateChanged, this, &MainWindow::updateSolverDescription);
    connect(m_dealer, &DealerScene::updateMoves, this, &MainWindow::slotUpdateMoves);

    m_solverStatusLabel->setText(QString());
    m_solverStatusLabel->setVisible(true);
    m_moveCountStatusLabel->setText(QString());
    m_moveCountStatusLabel->setVisible(true);

    updateActions();
    updateSoundEngine();
}

void MainWindow::slotShowGameSelectionScreen()
{
    if (!m_dealer || m_dealer->allowedToStartNewGame()) {
        if (m_dealer) {
            m_dealer->recordGameStatistics();
            delete m_dealer;
            m_view->setScene(nullptr);
            m_dealer = nullptr;
        }

        if (!m_selector) {
            m_selector = new GameSelectionScene(this);
            connect(m_selector, &GameSelectionScene::gameSelected, this, &MainWindow::slotGameSelected);
        }
        m_view->setScene(m_selector);

        m_gameHelpAction->setText(i18nc("@action Shown when there is no game open. Is always disabled.", "Help &with Current Game"));

        updateActions();

        setGameCaption();

        m_solverStatusLabel->setVisible(false);
        m_moveCountStatusLabel->setVisible(false);
    }
}

void MainWindow::updateActions()
{
    // Enable/disable application actions that aren't appropriate on game
    // selection screen.
    action(QStringLiteral("new_game"))->setEnabled(m_dealer);
    action(QStringLiteral("new_deal"))->setEnabled(m_dealer);
    action(QStringLiteral("game_restart"))->setEnabled(m_dealer);
    action(QStringLiteral("game_save_as"))->setEnabled(m_dealer);
    m_gameHelpAction->setEnabled(m_dealer);

    // Initially disable game actions. They'll be reenabled through signals
    // if/when appropriate.
    m_undoAction->setEnabled(false);
    m_redoAction->setEnabled(false);
    m_hintAction->setEnabled(false);
    m_demoAction->setEnabled(false);
    m_dropAction->setEnabled(false);
    m_drawAction->setEnabled(false);
    m_dealAction->setEnabled(false);
    m_redealAction->setEnabled(false);

    // If a dealer exists, connect the game actions to it.
    if (m_dealer) {
        connect(m_dealer, &DealerScene::undoPossible, m_undoAction, &QAction::setEnabled);
        connect(m_dealer, &DealerScene::redoPossible, m_redoAction, &QAction::setEnabled);

        connect(m_dealer, &DealerScene::hintActive, m_hintAction, &QAction::setChecked);
        connect(m_dealer, &DealerScene::gameInProgress, m_hintAction, &QAction::setEnabled);

        connect(m_dealer, &DealerScene::demoActive, this, &MainWindow::toggleDemoAction);
        connect(m_dealer, &DealerScene::gameInProgress, m_demoAction, &QAction::setEnabled);

        connect(m_dealer, &DealerScene::dropActive, m_dropAction, &QAction::setChecked);
        connect(m_dealer, &DealerScene::gameInProgress, m_dropAction, &QAction::setEnabled);

        connect(m_dealer, &DealerScene::gameInProgress, m_saveAction, &QAction::setEnabled);

        connect(m_leftAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusLeft);
        connect(m_rightAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusRight);
        connect(m_upAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusUp);
        connect(m_downAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusDown);
        connect(m_cancelAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusCancel);
        connect(m_pickUpSetDownAction, &QAction::triggered, m_dealer, &DealerScene::keyboardFocusSelect);

        connect(m_dealer, &DealerScene::newDeal, this, &MainWindow::newGame);

        if (m_dealer->actions() & DealerScene::Draw) {
            connect(m_drawAction, &QAction::triggered, m_dealer, &DealerScene::drawDealRowOrRedeal);
            connect(m_dealer, &DealerScene::newCardsPossible, m_drawAction, &QAction::setEnabled);
        } else if (m_dealer->actions() & DealerScene::Deal) {
            connect(m_dealAction, &QAction::triggered, m_dealer, &DealerScene::drawDealRowOrRedeal);
            connect(m_dealer, &DealerScene::newCardsPossible, m_dealAction, &QAction::setEnabled);
        } else if (m_dealer->actions() & DealerScene::Redeal) {
            connect(m_redealAction, &QAction::triggered, m_dealer, &DealerScene::drawDealRowOrRedeal);
            connect(m_dealer, &DealerScene::newCardsPossible, m_redealAction, &QAction::setEnabled);
        }

    }

    // Plug the per-game options at the top of the Settings menu.
    for (const QPointer<QAction> &a : std::as_const(m_pluggedDealerOptions)) {
        if (a)
            m_settingsMenu->removeAction(a);
    }
    m_pluggedDealerOptions.clear();
    if (m_dealer) {
        const QList<QAction *> options = m_dealer->configActions();
        m_settingsMenu->insertActions(m_settingsOptionsSeparator, options);
        for (QAction *a : options)
            m_pluggedDealerOptions << a;
    }
    m_settingsOptionsSeparator->setVisible(!m_pluggedDealerOptions.isEmpty());

    updateGameActionList();
}

void MainWindow::updateGameActionList()
{
    for (QAction *a : std::as_const(m_pluggedGameActions)) {
        m_moveMenu->removeAction(a);
        m_toolBar->removeAction(a);
    }
    m_pluggedGameActions.clear();

    m_dropAction->setEnabled(m_dealer && !m_dealer->autoDropEnabled());

    if (m_dealer) {
        QList<QAction *> actionList;
        if (m_dealer->actions() & DealerScene::Hint)
            actionList.append(m_hintAction);
        if (m_dealer->actions() & DealerScene::Demo)
            actionList.append(m_demoAction);
        if (m_dealer->actions() & DealerScene::Draw)
            actionList.append(m_drawAction);
        if (m_dealer->actions() & DealerScene::Deal)
            actionList.append(m_dealAction);
        if (m_dealer->actions() & DealerScene::Redeal)
            actionList.append(m_redealAction);
        if (!m_dealer->autoDropEnabled())
            actionList.append(m_dropAction);
        m_moveMenu->addActions(actionList);
        m_toolBar->addActions(actionList);
        m_pluggedGameActions = actionList;
    }
}

void MainWindow::updateSoundEngine()
{
    if (m_dealer) {
        if (Settings::playSounds()) {
            if (!m_soundEngine)
                m_soundEngine = new SoundEngine(this);

            connect(m_dealer, &DealerScene::cardsPickedUp, m_soundEngine, &SoundEngine::cardsPickedUp);
            connect(m_dealer, &DealerScene::cardsPutDown, m_soundEngine, &SoundEngine::cardsPutDown);
        } else if (m_soundEngine) {
            disconnect(m_dealer, nullptr, m_soundEngine, nullptr);
        }
    }
}

void MainWindow::toggleDrop()
{
    if (m_dealer) {
        if (!m_dealer->isDropActive())
            m_dealer->startDrop();
        else
            m_dealer->stopDrop();
    }
}

void MainWindow::toggleHints()
{
    if (m_dealer) {
        if (!m_dealer->isHintActive())
            m_dealer->startHint();
        else
            m_dealer->stop();
    }
}

void MainWindow::toggleDemo()
{
    if (m_dealer) {
        if (!m_dealer->isDemoActive())
            m_dealer->startDemo();
        else
            m_dealer->stop();
    }
}

void MainWindow::toggleDemoAction(bool active)
{
    m_demoAction->setChecked(active);
    m_demoAction->setIcon(QIcon::fromTheme(active ? QStringLiteral("media-playback-pause") : QStringLiteral("media-playback-start")));
}

void MainWindow::toggleMenubar()
{
    if (m_showMenubarAction->isChecked())
        menuBar()->show();
    else if (ConfirmDialog::warningContinueCancel(this,
                                                  i18n("Are you sure you want to hide the menubar? The current shortcut to show it again is %1.",
                                                       m_showMenubarAction->shortcut().toString(QKeySequence::NativeText)),
                                                  i18nc("@title:window", "Hide Menubar"),
                                                  i18nc("@action:button", "Continue"),
                                                  QStringLiteral("MenubarWarning")))
        menuBar()->hide();
    else
        m_showMenubarAction->setChecked(true);
}

void MainWindow::toggleFullscreen(bool fullScreen)
{
    if (fullScreen)
        setWindowState(windowState() | Qt::WindowFullScreen);
    else
        setWindowState(windowState() & ~Qt::WindowFullScreen);
    m_fullScreenAction->setIcon(QIcon::fromTheme(fullScreen ? QStringLiteral("view-restore") : QStringLiteral("view-fullscreen")));
    m_fullScreenAction->setText(fullScreen ? i18nc("@action", "Exit F&ull Screen Mode") : i18nc("@action", "F&ull Screen Mode"));
}

void MainWindow::closeEvent(QCloseEvent *e)
{
    QString stateDirName = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    QString stateFileName = stateDirName + QLatin1String("/" saved_state_file);
    QDir stateFileDir(stateDirName);
    if (!stateFileDir.exists()) {
        // create the directory if it doesn't exist (bug#350160)
        stateFileDir.mkpath(QStringLiteral("."));
    }
    QFile stateFile(stateFileName);

    // Remove the existing state file, if any.
    stateFile.remove();

    if (m_dealer) {
        if (Settings::rememberStateOnExit() && !m_dealer->isGameWon()) {
            if (stateFile.open(QFile::WriteOnly | QFile::Truncate))
                m_dealer->saveFile(&stateFile);
            else
                qCWarning(KPAT_LOG) << "Could not save the game state to" << stateFileName << ":" << stateFile.errorString();
        } else {
            // If there's a game in progress and we aren't going to save it
            // then record its statistics, since the DealerScene will be destroyed
            // shortly.
            m_dealer->recordGameStatistics();
        }
    }

    QSettings &config = Settings::config();
    config.setValue(windowGroup + QStringLiteral("Geometry"), saveGeometry());
    config.setValue(windowGroup + QStringLiteral("State"), saveState());
    config.setValue(windowGroup + QStringLiteral("MenuBarVisible"), !menuBar()->isHidden());
    config.setValue(windowGroup + QStringLiteral("StatusBarVisible"), !statusBar()->isHidden());
    config.sync();

    QMainWindow::closeEvent(e);
}

void MainWindow::newNumberedDeal()
{
    if (!m_dealDialog) {
        m_dealDialog = new NumberedDealDialog(this);
        connect(m_dealDialog, &NumberedDealDialog::dealChosen, this, &MainWindow::startNumbered);
    }

    if (m_dealer) {
        m_dealDialog->setGameType(m_dealer->oldId());
        m_dealDialog->setDealNumber(m_dealer->gameNumber());
    }

    m_dealDialog->show();
}

void MainWindow::startNumbered(int gameId, int dealNumber)
{
    if (!m_dealer || m_dealer->allowedToStartNewGame()) {
        setGameType(gameId);
        startNew(dealNumber);
    }
}

void MainWindow::nextDeal()
{
    if (!m_dealer)
        newNumberedDeal();
    else if (m_dealer->allowedToStartNewGame())
        startNew(m_dealer->gameNumber() == INT_MAX ? 1 : m_dealer->gameNumber() + 1);
}

void MainWindow::previousDeal()
{
    if (!m_dealer)
        newNumberedDeal();
    else if (m_dealer->allowedToStartNewGame())
        startNew(m_dealer->gameNumber() == 1 ? INT_MAX : m_dealer->gameNumber() - 1);
}

bool MainWindow::loadGame(const QUrl &url, bool addToRecentFiles)
{
    if (!url.isLocalFile()) {
        QMessageBox::critical(this, QString(), i18n("Downloading file failed: %1", i18nc("@info", "Only local files are supported.")));
        return false;
    }
    QFile inputFile(url.toLocalFile());
    if (!inputFile.open(QIODevice::ReadOnly)) {
        QMessageBox::critical(this, QString(), i18n("Downloading file failed: %1", inputFile.errorString()));
        return false;
    }
    const QByteArray fileData = inputFile.readAll();
    inputFile.close();

    QXmlStreamReader xml(fileData);
    if (!xml.readNextStartElement()) {
        QMessageBox::critical(this, QString(), i18n("Error reading XML file: ") + xml.errorString());
        return false;
    }

    int gameId = -1;
    bool isLegacyFile;

    if (xml.name() == QLatin1String("dealer")) {
        isLegacyFile = true;
        bool ok;
        int id = xml.attributes().value(QStringLiteral("id")).toString().toInt(&ok);
        if (ok)
            gameId = id;
    } else if (xml.name() == QLatin1String("kpat-game")) {
        isLegacyFile = false;
        gameId = DealerInfoList::self()->gameIdForFile(xml);
    } else {
        QMessageBox::critical(this, QString(), i18n("XML file is not a KPat save."));
        return false;
    }

    if (!m_dealer_map.contains(gameId)) {
        QMessageBox::critical(this, QString(), i18n("Unrecognized game id."));
        return false;
    }

    // Only bother the user to ask for permission after we've determined the
    // save game file is at least somewhat valid.
    if (m_dealer && !m_dealer->allowedToStartNewGame())
        return false;

    setGameType(gameId);

    xml.clear();

    QBuffer buffer;
    buffer.setData(fileData);
    buffer.open(QBuffer::ReadOnly);
    bool success = isLegacyFile ? m_dealer->loadLegacyFile(&buffer) : m_dealer->loadFile(&buffer);

    if (!success) {
        QMessageBox::critical(this, QString(), i18n("Errors encountered while parsing file."));
        slotShowGameSelectionScreen();
        return false;
    }

    setGameCaption();

    if (addToRecentFiles)
        m_recentFilesAction->addUrl(url);

    return true;
}

void MainWindow::loadGame()
{
    QPointer<QFileDialog> dialog = new QFileDialog(this);
    dialog->setDirectory(lastDirectory());
    dialog->setAcceptMode(QFileDialog::AcceptOpen);
    dialog->setNameFilters(QStringList() << saveFileFilter() << legacySaveFileFilter() << oldKPatienceFileFilter() << allFilesFilter());
    dialog->setWindowTitle(i18nc("@title:window", "Load Game"));

    if (dialog->exec() == QFileDialog::Accepted) {
        if (dialog) {
            QUrl url = dialog->selectedUrls().at(0);
            if (!url.isEmpty()) {
                setLastDirectory(url.toLocalFile());
                loadGame(url);
            }
        }
    }
    delete dialog;
}

void MainWindow::loadGame(const QUrl &url)
{
    loadGame(url, true);
}

void MainWindow::saveGame()
{
    if (!m_dealer)
        return;

    QPointer<QFileDialog> dialog = new QFileDialog(this);
    dialog->setDirectory(lastDirectory());
    dialog->setAcceptMode(QFileDialog::AcceptSave);
    dialog->setNameFilters(QStringList() << saveFileFilter() << legacySaveFileFilter());
    dialog->setDefaultSuffix(QStringLiteral("pspider"));
    connect(dialog.data(), &QFileDialog::filterSelected, dialog.data(), [d = dialog.data()](const QString &filter) {
        d->setDefaultSuffix(filter == legacySaveFileFilter() ? QStringLiteral("pspider-legacy") : QStringLiteral("pspider"));
    });
    dialog->setOption(QFileDialog::DontConfirmOverwrite, false);
    dialog->setWindowTitle(i18nc("@title:window", "Save Game"));
    if (dialog->exec() != QFileDialog::Accepted)
        return;

    QUrl url;
    if (dialog) {
        url = dialog->selectedUrls().at(0);
        if (url.isEmpty())
            return;
    }

    if (!url.isLocalFile()) {
        QMessageBox::critical(this, QString(), i18n("Error uploading file. Saving failed: %1", i18nc("@info", "Only local files are supported.")));
        return;
    }
    setLastDirectory(url.toLocalFile());

    QFile file(url.toLocalFile());
    if (!file.open(QFile::WriteOnly)) {
        QMessageBox::critical(this, QString(), i18n("Error opening file for writing. Saving failed."));
        return;
    }
    if (dialog && dialog->selectedNameFilter() == legacySaveFileFilter()) {
        m_dealer->saveLegacyFile(&file);
    } else {
        m_dealer->saveFile(&file);
    }
    file.close();

    m_recentFilesAction->addUrl(url);
}

void MainWindow::showStats()
{
    QPointer<StatisticsDialog> dlg = new StatisticsDialog(this);
    if (m_dealer)
        dlg->showGameType(m_dealer->oldId());
    dlg->exec();
    delete dlg;
}

void MainWindow::updateSolverDescription(const QString &text)
{
    m_solverStatusLabel->setText(text);
}

void MainWindow::slotUpdateMoves(int moves)
{
    m_moveCountStatusLabel->setText(i18np("1 move", "%1 moves", moves));
}

void MainWindow::slotSnapshot()
{
    if (m_dealer_it == m_dealer_map.constEnd()) {
        // first call
        m_dealer_it = m_dealer_map.constBegin();
    }

    setGameType(m_dealer_it.key());
    m_dealer->setAutoDropEnabled(false);
    startRandom();

    QTimer::singleShot(1000, this, &MainWindow::slotSnapshot2);
}

void MainWindow::slotSnapshot2()
{
    m_dealer->createDump().save(QStringLiteral("%1.png").arg(m_dealer->gameId()));

    ++m_dealer_it;
    if (m_dealer_it != m_dealer_map.constEnd())
        QTimer::singleShot(0, this, &MainWindow::slotSnapshot);
}

void MainWindow::generateThemePreview()
{
    const QSize previewSize(240, 160);
    m_view->setMinimumSize(previewSize);
    m_view->setMaximumSize(previewSize);

    QImage img(m_view->contentsRect().size(), QImage::Format_ARGB32);
    img.fill(Qt::transparent);
    QPainter p(&img);

    const auto cards = m_cardDeck->cards();
    for (KCard *c : cards)
        c->completeAnimation();

    m_view->render(&p);

    slotShowGameSelectionScreen();

    QRect leftHalf(0, 0, m_view->contentsRect().width() / 2, m_view->contentsRect().height());
    m_view->render(&p, leftHalf, leftHalf);

    p.end();

    img.save(QStringLiteral("preview.png"));

    m_view->setMinimumSize(0, 0);
    m_view->setMaximumSize(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX);
}

#include "moc_mainwindow.cpp"
