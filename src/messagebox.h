/*
 * Copyright (C) 2010 Parker Coates <coates@kde.org>
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License as
 * published by the Free Software Foundation; either version 2 of
 * the License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

#ifndef MESSAGEBOX_H
#define MESSAGEBOX_H

// Qt
#include <QFont>
#include <QGraphicsPixmapItem>

class MessageBox : public QGraphicsPixmapItem
{
public:
    MessageBox();

    void setMessage(const QString &message);
    QString message() const;

    void setSize(const QSize &size);
    QSize size() const;

    void paint(QPainter *painter, const QStyleOptionGraphicsItem *option, QWidget *widget) override;

private:
    QString m_message;
    QFont m_font;
    void renderFrame();

    QSize m_size;
    int m_themeGeneration;
    bool m_fontCached;
};

#endif
