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

#include "soundengine.h"

// own
#include "datapaths.h"
// Qt
#include <QAudioOutput>
#include <QMediaPlayer>
#include <QUrl>

SoundEngine::SoundEngine(QObject *parent)
    : QObject(parent)
    , m_cardPickedUp(createPlayer(QStringLiteral("sounds/card-pickup.ogg")))
    , m_cardPutDown(createPlayer(QStringLiteral("sounds/card-down.ogg")))
{
}

QMediaPlayer *SoundEngine::createPlayer(const QString &fileName)
{
    const QString path = DataPaths::locate(fileName);
    if (path.isEmpty())
        return nullptr;

    auto player = new QMediaPlayer(this);
    auto output = new QAudioOutput(player);
    player->setAudioOutput(output);
    player->setSource(QUrl::fromLocalFile(path));
    return player;
}

void SoundEngine::play(QMediaPlayer *player)
{
    if (!player)
        return;
    if (player->playbackState() == QMediaPlayer::PlayingState)
        player->setPosition(0);
    else
        player->play();
}

SoundEngine::~SoundEngine()
{
}

void SoundEngine::cardsPickedUp()
{
    play(m_cardPickedUp);
}

void SoundEngine::cardsPutDown()
{
    play(m_cardPutDown);
}

#include "moc_soundengine.cpp"
