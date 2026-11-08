#!/usr/bin/env bash
# Extracts the translatable strings of Packarch Spider into po/packarch-spider.pot
# (replaces KDE's Messages.sh, which needed the KDE "scripty" tooling).
#
# Update a translation afterwards with:
#   msgmerge --update po/<lang>/packarch-spider.po po/packarch-spider.pot
#
# SPDX-License-Identifier: GPL-2.0-or-later
set -euo pipefail
cd "$(dirname "$0")/.."

xgettext --from-code=UTF-8 -C \
    --package-name=packarch-spider \
    --msgid-bugs-address=https://github.com/packarch/packarch-spider/issues \
    -ci18n \
    -ki18n:1 -ki18nc:1c,2 -ki18np:1,2 -ki18ncp:1c,2,3 \
    -kki18n:1 -kki18nc:1c,2 -kkli18n:1 -kkli18nc:1c,2 \
    -ktr2i18n:1 \
    --add-comments=TRANSLATORS \
    -o po/packarch-spider.pot \
    $(find src -name '*.cpp' | sort)

# Strings of the .ui files (uic generates tr2i18n() calls at build time).
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
for ui in $(find src -name '*.ui' | sort); do
    sed -n 's|.*<string[^>]*>\(.*\)</string>.*|tr2i18n("\1");|p' "$ui" | sed 's/&amp;/\&/g; s/&lt;/</g; s/&gt;/>/g' >> "$tmp"
done
if [ -s "$tmp" ]; then
    xgettext --from-code=UTF-8 -C -j -ktr2i18n:1 -o po/packarch-spider.pot --language=C++ "$tmp"
fi

echo "po/packarch-spider.pot: $(grep -c '^msgid ' po/packarch-spider.pot) strings"
