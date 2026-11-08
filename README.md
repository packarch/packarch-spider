# Packarch Spider

Patience (solitaire) card games for [Packarch](https://github.com/packarch).

Packarch Spider is based on [KPatience](https://apps.kde.org/kpat/), the
patience game of the KDE project. It has been ported to build with **Qt 6
only**: it does not need KDE Frameworks, libkdegames or extra-cmake-modules.

15 game types: Klondike, Spider, Freecell, Golf, Castle, Yukon, Baker's Dozen,
Bizilizi, Simple Simon, Forty & Eight, Grandfather, Grandfather's Clock, Mod3,
Normandy and Aces Up.

## Dependencies

| Dependency | Purpose |
|---|---|
| Qt ≥ 6.5: Widgets, Svg, Xml, Multimedia | everything |
| [freecell-solver](https://fc-solve.shlomifish.org/) (required) | hints, "can this game be won?" |
| [black-hole-solver](https://www.shlomifish.org/open-source/projects/black-hole-solitaire-solver/) (optional) | solver for Golf |
| gettext (`msgfmt`, optional) | translations |
| CMake ≥ 3.16, a C++17 compiler | build |

Arch Linux:

```sh
sudo pacman -S --needed cmake qt6-base qt6-svg qt6-multimedia freecell-solver black-hole-solver
```

Debian / Ubuntu:

```sh
sudo apt install cmake g++ qt6-base-dev qt6-svg-dev qt6-multimedia-dev \
    libfreecell-solver-dev libblack-hole-solver-dev gettext
```

## Build and run

```sh
cmake -S . -B build
cmake --build build -j
./build/bin/packarch-spider # runs straight from the build tree
sudo cmake --install build  # optional, installs to /usr/local
```

CMake options:

- `-DWITH_BH_SOLVER=OFF`: do not use black-hole-solver
- `-DBUILD_TRANSLATIONS=OFF`: do not compile the translations
- `-DBUILD_TESTING=ON`: build the autotests (`ctest --test-dir build`)

## Identity

| | |
|---|---|
| Binary | `packarch-spider` |
| Application id / desktop file | `org.packarch.Spider` |
| Data | `<prefix>/share/packarch-spider/` |
| Translations | `<prefix>/share/locale/*/LC_MESSAGES/packarch-spider{,-extra}.mo` |
| Settings and statistics | `~/.config/packarch-spider/packarch-spider.conf` |
| "Remember State on Exit" save | `~/.local/share/packarch-spider/` |
| Saved games | `*.pspider` (`application/x-packarch-spider-savedgame`) |

Saved games of KPatience (`*.kpat`, `*.kpatience`) can be opened too.

## Differences from KPatience

- Card decks and themes cannot be downloaded from the application. The decks
  in `carddecks/` (copied from libkdegames) are included. Decks installed by KDE
  in `/usr/share/carddecks` are used too.
- Only local files can be opened and saved.
- The help menu opens the online KPatience handbook.

## Source layout

- `src/compat/`: small Qt replacements for the KDE classes KPatience used:
  gettext-based `i18n()`, `.desktop` file reader, image cache, select and
  recent-files actions, data file lookup.
- `src/libkcardgame/`: card, pile, deck and scene library.
- `src/patsolve/`: solvers.
- `src/renderer.*`: SVG game theme renderer (replaces KGameRenderer).
- `src/settings.*`: settings (replaces the kconfig_compiler generated class).
- `themes/`, `carddecks/`, `previews/`, `sounds/`: game data.

## Translations

`po/<lang>/packarch-spider.po` are the KPatience catalogs written by the KDE
translation teams. `po/<lang>/packarch-spider-extra.po` hold the standard
strings (menus, Quit, Undo…) that used to come from KDE Frameworks, and the few
strings specific to Packarch Spider. They are generated from the official KDE
catalogs by `tools/update-extra-translations.py`. Hand-made translations are
set in that script (`MANUAL`).

To extract the strings into `po/packarch-spider.pot`, run `tools/extract-messages.sh`.

## License

GPL-2.0-or-later, see `COPYING`. KPatience is © its authors (Paul Olav Tvete,
Stephan Kulow, Parker Coates, Shlomi Fish and many others, see the About
dialog). The card decks and themes keep their original licenses (see
`carddecks/*/COPYING`, `COPYRIGHT`, `AUTHORS`).
