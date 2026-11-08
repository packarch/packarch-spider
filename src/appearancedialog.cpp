/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "appearancedialog.h"

// own
#include "i18n.h"
#include "kpat_version.h"
#include "renderer.h"
#include "settings.h"
// KCardGame
#include "kcardthemewidget.h"
// Qt
#include <QDialogButtonBox>
#include <QIcon>
#include <QListWidget>
#include <QPushButton>
#include <QTabWidget>
#include <QVBoxLayout>

namespace
{
const int ThemeIdRole = Qt::UserRole + 1;
}

AppearanceDialog::AppearanceDialog(QWidget *parent)
    : QDialog(parent)
{
    // Reuse the "&Configure %1…" action text as the window title.
    QString title = i18nc("@action", "&Configure %1…", QStringLiteral(KPAT_APP_NAME));
    title.remove(QLatin1Char('&'));
    if (title.endsWith(QChar(0x2026)))
        title.chop(1);
    else if (title.endsWith(QLatin1String("...")))
        title.chop(3);
    setWindowTitle(title.trimmed());

    const QString previewFormat = QStringLiteral("back;10_spade,jack_diamond,queen_club,king_heart;1_spade");
    const QSet<QString> features = QSet<QString>() << QStringLiteral("AngloAmerican") << QStringLiteral("Backs1");

    auto tabs = new QTabWidget(this);

    m_cardThemeWidget = new KCardThemeWidget(features, previewFormat, this);
    tabs->addTab(m_cardThemeWidget, QIcon::fromTheme(QStringLiteral("games-config-theme")), i18nc("@title:tab", "Card Deck"));

    m_themeList = new QListWidget(this);
    m_themeList->setViewMode(QListView::ListMode);
    m_themeList->setIconSize(QSize(160, 107));
    m_themeList->setSpacing(4);
    m_themeList->setAlternatingRowColors(true);
    const auto themes = Renderer::self()->themes();
    for (const GameTheme &theme : themes) {
        QString text = theme.name;
        if (!theme.author.isEmpty())
            text += QLatin1Char('\n') + i18nc("@info", "Author:") + QLatin1Char(' ') + theme.author;
        if (!theme.description.isEmpty())
            text += QLatin1Char('\n') + theme.description;
        auto item = new QListWidgetItem(QIcon(theme.previewPath), text, m_themeList);
        item->setData(ThemeIdRole, theme.id);
    }
    tabs->addTab(m_themeList, QIcon::fromTheme(QStringLiteral("games-config-theme")), i18nc("@title:tab", "Game Theme"));

    m_buttonBox = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Apply | QDialogButtonBox::Cancel, this);
    connect(m_buttonBox, &QDialogButtonBox::accepted, this, [this]() {
        applySettings();
        accept();
    });
    connect(m_buttonBox, &QDialogButtonBox::rejected, this, &QDialog::reject);
    connect(m_buttonBox->button(QDialogButtonBox::Apply), &QPushButton::clicked, this, &AppearanceDialog::applySettings);

    connect(m_cardThemeWidget, &KCardThemeWidget::currentSelectionChanged, this, &AppearanceDialog::updateButtons);
    connect(m_themeList, &QListWidget::currentItemChanged, this, &AppearanceDialog::updateButtons);

    auto layout = new QVBoxLayout(this);
    layout->addWidget(tabs);
    layout->addWidget(m_buttonBox);

    loadSettings();
    resize(700, 550);
}

void AppearanceDialog::loadSettings()
{
    m_cardThemeWidget->setCurrentSelection(Settings::cardTheme());

    const QString currentTheme = Renderer::self()->currentTheme().id;
    for (int i = 0; i < m_themeList->count(); ++i) {
        if (m_themeList->item(i)->data(ThemeIdRole).toString() == currentTheme) {
            m_themeList->setCurrentRow(i);
            break;
        }
    }
    updateButtons();
}

void AppearanceDialog::applySettings()
{
    const QString cardTheme = m_cardThemeWidget->currentSelection();
    if (!cardTheme.isEmpty())
        Settings::setCardTheme(cardTheme);

    if (QListWidgetItem *item = m_themeList->currentItem())
        Renderer::self()->setCurrentTheme(item->data(ThemeIdRole).toString());

    Settings::self()->save();
    updateButtons();
    Q_EMIT settingsChanged();
}

void AppearanceDialog::updateButtons()
{
    bool changed = false;
    const QString cardTheme = m_cardThemeWidget->currentSelection();
    if (!cardTheme.isEmpty() && cardTheme != Settings::cardTheme())
        changed = true;
    if (QListWidgetItem *item = m_themeList->currentItem()) {
        if (item->data(ThemeIdRole).toString() != Renderer::self()->currentTheme().id)
            changed = true;
    }
    m_buttonBox->button(QDialogButtonBox::Apply)->setEnabled(changed);
}

#include "moc_appearancedialog.cpp"
