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
 */

#include "renderer.h"

// own
#include "datapaths.h"
#include "desktopfile.h"
#include "kpat_debug.h"
#include "settings.h"
// Qt
#include <QDir>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImage>
#include <QPainter>
#include <QSvgRenderer>

namespace
{
const QString defaultThemeId = QStringLiteral("greenblaze");
const int maxCachedPixmaps = 200;
}

Renderer *Renderer::s_instance = nullptr;

Renderer *Renderer::self()
{
    if (s_instance == nullptr)
        s_instance = new Renderer();
    return s_instance;
}

void Renderer::deleteSelf()
{
    delete s_instance;
    s_instance = nullptr;
    Settings::self()->save();
}

Renderer::Renderer()
    : m_svg(new QSvgRenderer())
    , m_generation(0)
{
    discoverThemes();

    GameTheme theme;
    const QString wanted = Settings::theme();
    for (const GameTheme &t : std::as_const(m_themes)) {
        if (t.id == wanted)
            theme = t;
        else if (!theme.isValid() && t.id == defaultThemeId)
            theme = t;
    }
    if (!theme.isValid() && !m_themes.isEmpty())
        theme = m_themes.first();

    if (!theme.isValid())
        qCWarning(KPAT_LOG) << "No game theme found. Searched in" << DataPaths::dataDirs();

    loadTheme(theme);
}

Renderer::~Renderer()
{
    delete m_svg;
}

void Renderer::discoverThemes()
{
    m_themes.clear();
    QStringList seen;
    const QStringList dirs = DataPaths::locateAllDirs(QStringLiteral("themes"));
    for (const QString &dirPath : dirs) {
        QDir dir(dirPath);
        const auto entries = dir.entryList(QStringList() << QStringLiteral("*.desktop"), QDir::Files, QDir::Name);
        for (const QString &entry : entries) {
            const QString id = QFileInfo(entry).completeBaseName();
            if (seen.contains(id))
                continue;

            DesktopFile file(dir.filePath(entry));
            const QString group = QStringLiteral("KGameTheme");
            if (!file.hasGroup(group))
                continue;

            GameTheme theme;
            theme.id = id;
            theme.name = file.localizedValue(group, QStringLiteral("Name"), id);
            theme.description = file.localizedValue(group, QStringLiteral("Description"));
            theme.author = file.value(group, QStringLiteral("Author"));

            QString graphics = file.value(group, QStringLiteral("FileName"));
            QFileInfo graphicsInfo(dir, graphics);
            // Sources ship plain .svg files, installed themes may use .svgz.
            if (!graphicsInfo.exists() && graphics.endsWith(QLatin1String(".svgz")))
                graphicsInfo = QFileInfo(dir, graphics.chopped(1));
            if (!graphicsInfo.exists())
                continue;
            theme.graphicsPath = graphicsInfo.absoluteFilePath();

            const QString preview = file.value(group, QStringLiteral("Preview"));
            if (!preview.isEmpty() && QFileInfo::exists(dir.filePath(preview)))
                theme.previewPath = dir.filePath(preview);

            seen << id;
            m_themes << theme;
        }
    }
}

QList<GameTheme> Renderer::themes() const
{
    return m_themes;
}

GameTheme Renderer::currentTheme() const
{
    return m_current;
}

void Renderer::setCurrentTheme(const QString &id)
{
    if (id == m_current.id)
        return;
    for (const GameTheme &t : std::as_const(m_themes)) {
        if (t.id == id) {
            loadTheme(t);
            Settings::setTheme(id);
            Q_EMIT themeChanged();
            return;
        }
    }
}

void Renderer::loadTheme(const GameTheme &theme)
{
    m_current = theme;
    m_pixmapCache.clear();
    m_colors.clear();
    ++m_generation;

    if (theme.isValid() && !m_svg->load(theme.graphicsPath))
        qCWarning(KPAT_LOG) << "Could not load theme graphics" << theme.graphicsPath;
}

QImage Renderer::renderImage(const QString &key, const QSize &pixelSize)
{
    QImage image(pixelSize, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    if (m_svg->isValid() && m_svg->elementExists(key)) {
        QPainter painter(&image);
        m_svg->render(&painter, key, QRectF(QPointF(0, 0), pixelSize));
    }
    return image;
}

QPixmap Renderer::spritePixmap(const QString &key, const QSize &size)
{
    if (size.isEmpty())
        return QPixmap();

    const qreal dpr = qApp ? qApp->devicePixelRatio() : 1.0;
    const QSize pixelSize = (QSizeF(size) * dpr).toSize();
    const QString cacheKey = QStringLiteral("%1_%2x%3").arg(key).arg(pixelSize.width()).arg(pixelSize.height());

    auto it = m_pixmapCache.constFind(cacheKey);
    if (it != m_pixmapCache.constEnd())
        return *it;

    if (m_pixmapCache.size() > maxCachedPixmaps)
        m_pixmapCache.clear();

    QPixmap pixmap = QPixmap::fromImage(renderImage(key, pixelSize));
    pixmap.setDevicePixelRatio(dpr);
    m_pixmapCache.insert(cacheKey, pixmap);
    return pixmap;
}

QRectF Renderer::boundsOnSprite(const QString &key)
{
    if (!m_svg->isValid() || !m_svg->elementExists(key))
        return QRectF();
    return m_svg->transformForElement(key).mapRect(m_svg->boundsOnElement(key));
}

bool Renderer::spriteExists(const QString &key)
{
    return m_svg->isValid() && m_svg->elementExists(key);
}

qreal Renderer::aspectRatioOfElement(const QString &elementId)
{
    const QSizeF size = boundsOnSprite(elementId).size();
    if (size.height() <= 0)
        return 1.0;
    return size.width() / size.height();
}

QColor Renderer::colorOfElement(const QString &elementId)
{
    auto it = m_colors.constFind(elementId);
    if (it != m_colors.constEnd())
        return *it;

    // Like KGameRenderer, ignore the alpha channel: themes that lack the
    // element (none of the bundled ones defines "message_text_color")
    // then get opaque black instead of an invisible, fully transparent color.
    const QImage image = renderImage(elementId, QSize(3, 3));
    const QColor color = QColor(QRgb(image.pixel(1, 1)) | 0xff000000u);
    m_colors.insert(elementId, color);
    return color;
}

#include "moc_renderer.cpp"
