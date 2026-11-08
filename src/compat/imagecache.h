/*
 * Thread-safe in-memory cache for rendered images and small serialized
 * values. Replaces KImageCache / KSharedDataCache (which were shared,
 * memory-mapped caches) with a simple per-process store.
 *
 * SPDX-License-Identifier: GPL-2.0-or-later
 */

#ifndef KPAT_COMPAT_IMAGECACHE_H
#define KPAT_COMPAT_IMAGECACHE_H

#include <QByteArray>
#include <QHash>
#include <QImage>
#include <QMutex>
#include <QPixmap>
#include <QString>

class ImageCache
{
public:
    explicit ImageCache(const QString &name, qint64 maxBytes);

    bool find(const QString &key, QByteArray *destination) const;
    bool insert(const QString &key, const QByteArray &data);

    bool findImage(const QString &key, QImage *destination) const;
    bool findPixmap(const QString &key, QPixmap *destination) const;
    bool insertImage(const QString &key, const QImage &image);

    void clear();

    unsigned timestamp() const;
    void setTimestamp(unsigned timestamp);

private:
    void evictIfNeeded();

    struct Entry {
        QByteArray data;
        QImage image;
        quint64 lastUse = 0;
        qint64 cost = 0;
    };

    QString m_name;
    qint64 m_maxBytes;
    qint64 m_usedBytes = 0;
    unsigned m_timestamp = 0;
    mutable quint64 m_useCounter = 0;
    mutable QHash<QString, Entry> m_entries;
    mutable QMutex m_mutex;
};

#endif
