/*
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#include "imagecache.h"

#include <QMutexLocker>

ImageCache::ImageCache(const QString &name, qint64 maxBytes)
    : m_name(name)
    , m_maxBytes(maxBytes)
{
}

bool ImageCache::find(const QString &key, QByteArray *destination) const
{
    QMutexLocker locker(&m_mutex);
    auto it = m_entries.find(key);
    if (it == m_entries.end() || !it->image.isNull())
        return false;
    it->lastUse = ++m_useCounter;
    if (destination)
        *destination = it->data;
    return true;
}

bool ImageCache::insert(const QString &key, const QByteArray &data)
{
    QMutexLocker locker(&m_mutex);
    Entry &entry = m_entries[key];
    m_usedBytes -= entry.cost;
    entry.data = data;
    entry.image = QImage();
    entry.cost = data.size();
    entry.lastUse = ++m_useCounter;
    m_usedBytes += entry.cost;
    evictIfNeeded();
    return true;
}

bool ImageCache::findImage(const QString &key, QImage *destination) const
{
    QMutexLocker locker(&m_mutex);
    auto it = m_entries.find(key);
    if (it == m_entries.end() || it->image.isNull())
        return false;
    it->lastUse = ++m_useCounter;
    if (destination)
        *destination = it->image;
    return true;
}

bool ImageCache::findPixmap(const QString &key, QPixmap *destination) const
{
    QImage image;
    if (!findImage(key, &image))
        return false;
    if (destination)
        *destination = QPixmap::fromImage(image);
    return true;
}

bool ImageCache::insertImage(const QString &key, const QImage &image)
{
    if (image.isNull())
        return false;
    QMutexLocker locker(&m_mutex);
    Entry &entry = m_entries[key];
    m_usedBytes -= entry.cost;
    entry.data.clear();
    entry.image = image;
    entry.cost = image.sizeInBytes();
    entry.lastUse = ++m_useCounter;
    m_usedBytes += entry.cost;
    evictIfNeeded();
    return true;
}

void ImageCache::clear()
{
    QMutexLocker locker(&m_mutex);
    m_entries.clear();
    m_usedBytes = 0;
}

unsigned ImageCache::timestamp() const
{
    QMutexLocker locker(&m_mutex);
    return m_timestamp;
}

void ImageCache::setTimestamp(unsigned timestamp)
{
    QMutexLocker locker(&m_mutex);
    m_timestamp = timestamp;
}

void ImageCache::evictIfNeeded()
{
    // Called with the mutex held. Evict least recently used entries.
    while (m_usedBytes > m_maxBytes && m_entries.size() > 1) {
        auto oldest = m_entries.begin();
        for (auto it = m_entries.begin(); it != m_entries.end(); ++it) {
            if (it->lastUse < oldest->lastUse)
                oldest = it;
        }
        m_usedBytes -= oldest->cost;
        m_entries.erase(oldest);
    }
}
