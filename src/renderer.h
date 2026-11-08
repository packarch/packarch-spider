/*
 *  Copyright 2010 Parker Coates <coates@kde.org>
 *
 *  This program is free software; you can redistribute it and/or
 *  modify it under the terms of the GNU General Public License as
 *  published by the Free Software Foundation; either version 2 of
 *  the License, or (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 *  Rewritten without KDEGames: renders the SVG game theme (background,
 *  piles, bubbles, message box) with QSvgRenderer and keeps a small
 *  pixmap cache. Also handles theme discovery.
 */

#ifndef RENDERER_H
#define RENDERER_H

// Qt
#include <QColor>
#include <QHash>
#include <QList>
#include <QObject>
#include <QPixmap>
#include <QString>

class QSvgRenderer;

struct GameTheme {
    QString id; // file base name, e.g. "greenblaze"
    QString name;
    QString description;
    QString author;
    QString graphicsPath;
    QString previewPath;

    bool isValid() const
    {
        return !id.isEmpty() && !graphicsPath.isEmpty();
    }
};

class Renderer : public QObject
{
    Q_OBJECT

public:
    static Renderer *self();
    static void deleteSelf();

    // Available themes (found in <datadir>/kpat/themes/*.desktop).
    QList<GameTheme> themes() const;
    GameTheme currentTheme() const;
    void setCurrentTheme(const QString &id);

    // Pixmap of SVG element @p key, at logical size @p size, rendered for the
    // device pixel ratio of the screen.
    QPixmap spritePixmap(const QString &key, const QSize &size);
    QRectF boundsOnSprite(const QString &key);
    bool spriteExists(const QString &key);

    qreal aspectRatioOfElement(const QString &elementId);
    QColor colorOfElement(const QString &elementId);

    // Incremented every time the theme changes; lets non-QObject clients
    // notice that they need to re-render.
    int themeGeneration() const
    {
        return m_generation;
    }

Q_SIGNALS:
    void themeChanged();

private:
    Renderer();
    ~Renderer() override;

    void discoverThemes();
    void loadTheme(const GameTheme &theme);
    QImage renderImage(const QString &key, const QSize &pixelSize);

    static Renderer *s_instance;

    QList<GameTheme> m_themes;
    GameTheme m_current;
    QSvgRenderer *m_svg;
    QHash<QString, QPixmap> m_pixmapCache;
    QHash<QString, QColor> m_colors;
    int m_generation;
};

#endif
