# packarch-spider

Le **Spider**, le **FreeCell**, le **Klondike**, **l'horloge du grand-père**, **Simon le simple**, le
**Golf**, le **Yukon**, **Grand-père**, **Mod3**, **les As**, **Quarante & Huit** et **Treize à la douzaine** (jeux de patience), en Rust ([egui](https://github.com/emilk/egui)),
pour [Packarch](https://github.com/packarch/CachyOS-Packarch).

C'est un portage de jeux de [KPatience](https://apps.kde.org/kpat/) (KDE), dont il reprend
les règles, la numérotation des donnes, les jeux de cartes et les thèmes. Il remplace, depuis
la version 2.0.0, l'ancien Packarch Spider en C++/Qt (fork de KPatience, jusqu'à la
[version 1.0.1](https://github.com/packarch/packarch-spider/tree/v1.0.1)).
Comme dans KPatience, **Nouvelle partie…** (Ctrl+Maj+N, et au lancement s'il n'y a pas de
partie gardée) affiche l'écran de choix du jeu : une bulle par jeu, avec son aperçu (souris, ou
flèches et Entrée) ; **Nouvelle donne** (Ctrl+N) redistribue le même jeu.

## Le Spider

- 104 cartes en **1, 2, 3 ou 4 couleurs** (menu Configuration → Couleurs) ; les cartes des
  colonnes peuvent être distribuées face cachée (normal) ou face visible (plus facile).
- On déplace une suite de même couleur, de rangs décroissants, sur une carte de rang
  supérieur d'un, ou sur une colonne vide ; une suite Roi → As complète sort toute seule.
  Un clic sur la réserve (en bas à droite) distribue une rangée.
- Glisser-déposer, ou **double-clic** pour envoyer une suite à la meilleure place.
- **Annuler / Refaire** (Ctrl+Z, Ctrl+Maj+Z), **Nouvelle donne** (Ctrl+N), **Donne numérotée**
  (Ctrl+D), **Recommencer** (F5), **Statistiques** par nombre de couleurs.
- **Barre d'outils** comme celle de KPatience (Nouvelle partie, Nouvelle donne, Recommencer, Annuler, Refaire,
  Conseil, Démonstration, Distribuer) ; icônes seules (par défaut) ou icônes et texte,
  ou masquée (menu Configuration → Barre d'outils).
- **Outil de résolution** : il cherche en arrière-plan, après chaque coup, si la partie peut
  encore être gagnée (barre d'état). Comme celui de KPatience, il connaît les cartes cachées.
  Il donne exactement les mêmes résultats que l'original, mais répartit le travail entre
  plusieurs cœurs (la moitié, 8 au plus ; `PACKARCH_SOLVER_THREADS` pour forcer un nombre) et
  ne construit que les positions qu'il garde : 4 à 5 fois plus rapide (≈ 0,1 s par donne
  sur un Ryzen 9 7950X).
- **Conseil** (H) : le prochain coup gagnant quand l'outil de résolution le connaît (la carte
  et sa destination, ou la réserve), sinon toutes les cartes qui peuvent être déplacées.
- **Démonstration** (D) : joue les coups gagnants.
- Jeux de cartes et thèmes de KDE (7 jeux de cartes, 4 thèmes) ; ceux installés par KDE dans
  `/usr/share/carddecks` sont aussi proposés.
- Les mêmes numéros de donne donnent les mêmes parties que dans KPatience et Packarch Spider.

## Le FreeCell

- Les variantes de KPatience (menu Configuration → Variante) : **FreeCell**, **Baker's Game**
  (suites de même couleur), **Eight Off** (8 cellules dont 4 remplies, Rois seuls dans les
  colonnes vides), **Forecell** (cellules remplies, Rois seuls dans les colonnes vides),
  **Seahaven Towers** (10 colonnes, 2 cellules remplies, même couleur, Rois seuls) ;
- ou chaque règle à la main (variante personnalisée) : remplissage des colonnes vides
  (n'importe quelle carte, Rois seulement, aucune), suites (couleurs alternées, même
  couleur, rang seul), 0 à 8 cellules libres, 6 à 12 colonnes, 1 à 3 paquets ;
- déplacement de plusieurs cartes d'un coup quand les cellules et colonnes vides le
  permettent ((cellules libres + 1) × 2^colonnes vides, comme KPatience) ;
- **dépôt automatique** des cartes qui ne servent plus sur les fondations (règle de
  Raymond, comme KPatience) ; on peut le couper (Configuration) et utiliser **Déposer** (P) ;
- double-clic : vers une fondation, sinon dans une cellule libre ;
- annuler/refaire, conseil, démonstration, statistiques par variante, donnes numérotées
  (mêmes numéros que dans KPatience) comme pour le Spider ;
- **outil de résolution** : KPatience utilise pour le FreeCell une bibliothèque externe
  (Freecell Solver) ; celui-ci est écrit pour ce portage (recherche « meilleur d'abord » sur
  des positions normalisées). Il résout la plupart des donnes en quelques millisecondes et,
  quand il dit qu'une partie est perdue, il a essayé tous les coups possibles.

## Le Klondike

- Les règles de KPatience : 7 colonnes (cartes cachées sauf la dernière), talon en haut à
  gauche ; un clic sur le talon retourne **1 ou 3 cartes** sur la pioche (Configuration →
  Pioche) ; talon vide, un clic y retourne la pioche (sans limite) ;
- couleurs alternées dans les colonnes, Rois seulement dans une colonne vide ; en pioche 1,
  on peut reprendre la carte du dessus d'une fondation ;
- dépôt automatique (comme KPatience, il pioche aussitôt quand il a vidé la pioche) ou
  **Déposer** (P) ; **Piocher** (Tab) ; double-clic vers une fondation, sinon une colonne ;
- annuler/refaire, conseil, démonstration, statistiques et donnes numérotées par variante ;
- **outil de résolution** : portage de celui de KPatience (`patsolve` et `KlondikeSolver`) ;
  comme lui, il connaît les cartes cachées et l'ordre du talon. Sur les donnes 1 à 1000, il en
  gagne 877 en pioche 1 et 782 en pioche 3 ; les autres sont perdues ou indéterminées (limite
  d'un million de positions, 0,6 s au pire).

## L'horloge du grand-père

- Les règles de KPatience : 12 cartes forment le cadran (les fondations, de 9♦ à 8♣), les
  40 autres vont dans 8 piles de 5, toutes visibles ;
- les fondations montent dans leur couleur et repassent du Roi à l'As ; seule la carte du
  dessus d'une pile se déplace, sur une fondation, sur une carte d'un rang au-dessus (couleur
  libre) ou sur une pile vide ; gagné quand les 8 piles sont vides ;
- pas de dépôt automatique (comme KPatience) ; double-clic vers le cadran ;
- **outil de résolution** : portage de `ClockSolver` de KPatience, sur le même moteur
  `patsolve` que le Klondike ; il gagne 97 des donnes 1 à 100, en 2 ms en moyenne.

## Simon le simple

- Les règles de KPatience : 52 cartes visibles en 10 colonnes (8, 8, 8, 7, 6, 5, 4, 3, 2 et
  1 carte) ; on pose sur une carte d'un rang au-dessus (couleur libre) ou sur une colonne vide ;
- une suite descendante se déplace si elle a au plus 2^(colonnes vides) tronçons de même
  couleur ; seule une suite complète Roi → As d'une couleur monte sur une fondation (toute
  seule, dépôt automatique ; ou **Déposer**, P) ;
- double-clic : la suite va à la meilleure place (même couleur d'abord) ;
- **outil de résolution** : KPatience utilise Freecell Solver (bibliothèque externe) ; celui-ci
  est écrit pour ce portage, comme celui du FreeCell. Sur les donnes 1 à 1000 : 943 gagnées,
  56 perdues (il a essayé tous les coups), 1 indéterminée, 6 ms en moyenne.

## Le Golf

- Les règles de KPatience : 7 colonnes de 5 cartes visibles, le reste au talon ; la carte du
  dessus d'une colonne va sur le rebut si son rang suit ou précède celui du dessus du rebut
  (couleur libre, pas de passage du Roi à l'As) ; le talon (clic ou Tab) retourne une carte
  sur le rebut, sans se refaire ; colonnes vides, le reste du talon se retourne tout seul ;
- un clic sur une carte de colonne la joue (comme dans KPatience) ;
- **outil de résolution** : KPatience utilise black-hole-solver (bibliothèque externe) ;
  celui-ci est écrit pour ce portage : recherche exhaustive sur une position réduite à 30 bits.
  Sur les donnes 1 à 1000 : 463 gagnées, 537 perdues, aucune indéterminée (≈ 24 ms par donne,
  0,16 s au pire). Mêmes résultats que KPatience sur les 885 donnes qu'il tranche ; il laisse
  les 115 autres indéterminées.

## Le Yukon

- Les règles de KPatience : 7 colonnes (1 carte, puis j cartes cachées sous 5 visibles), pas de
  talon ; on descend en alternant rouge et noir, une colonne vide ne reçoit qu'un Roi ;
  n'importe quelle carte visible se déplace avec toutes celles qui sont dessus ; 4 fondations
  de l'As au Roi, sans retour ; dépôt automatique des sorties sans risque (règle de Raymond) ;
- **outil de résolution** : portage de `YukonSolver` de KPatience sur le moteur `patsolve`
  (comme lui, il connaît les cartes cachées). Sur les donnes 1 à 1000 : 727 gagnées,
  47 perdues, 226 indéterminées (limite d'un million de positions ; ≈ 0,15 s par donne, 1 s au
  pire). KPatience : 714 / 45 / 241 ; aucune donne tranchée différemment.

## Grand-père

- Les règles de KPatience (Grandfather) : 7 colonnes distribuées en serpentin, cartes cachées
  et visibles mêlées ; on descend dans la même couleur, une colonne vide ne reçoit qu'un Roi ;
  n'importe quelle carte visible se déplace avec celles qui sont dessus ; 4 fondations de l'As
  au Roi, sans retour ; deux redistributions (**Redistribuer**, R) ; toute sortie possible se
  fait toute seule (dépôt automatique) ;
- **outil de résolution** : portage de `GrandfSolver` de KPatience sur le moteur `patsolve`.
  Sur les donnes 1 à 1000 : 997 gagnées, 2 perdues, 1 indéterminée (5 s pour les 1000, 0,3 s
  au pire) : exactement les résultats de KPatience.

## Mod3

- Les règles de KPatience : 104 cartes (2 jeux) ; 3 rangées de fondations qui se montent de 3
  en 3 dans la même couleur (2, 5, 8, Valet ; 3, 6, 9, Dame ; 4, 7, 10, Roi), une rangée de
  réserve qui se remplit seule depuis le talon, une pile pour les As (dépôt automatique) ;
  **Distribuer** (Entrée, ou un clic sur le talon) pose une carte sur chaque pile de réserve ;
- **outil de résolution** : portage de `Mod3Solver` de KPatience sur le moteur `patsolve`.
  Sur les donnes 1 à 1000 : 747 gagnées, 19 perdues, 234 indéterminées (≈ 0,5 s par donne,
  2,6 s au pire, 245 Mo au pic). KPatience, avec une limite plus basse : 541 / 18 / 441 ;
  aucune donne tranchée différemment.

## Les As

- Les règles de KPatience (Aces Up) : 4 colonnes d'une carte, le reste au talon ; la carte du
  dessus d'une colonne est écartée si une autre colonne montre une carte plus haute de la
  même couleur (l'As est la plus haute) ; une colonne vide reçoit la carte du dessus de
  n'importe quelle autre ; **Distribuer** (Entrée, ou un clic sur le talon) pose une carte sur
  chaque colonne ; gagné quand il ne reste que les 4 As, un par colonne ;
- un clic sur une carte de colonne la joue (comme dans KPatience) ;
- **outil de résolution** : celui de KPatience (`IdiotSolver`) ne pose pas les cartes
  distribuées sur les colonnes, et il écarte d'office toute carte qui peut l'être, ce qui
  peut perdre une partie gagnable (donne 34 : il faut garder une carte pour battre plus tard
  une carte plus basse de sa couleur). Celui-ci est écrit pour ce portage : une recherche
  rapide qui écarte tout d'office, puis, si elle échoue, une recherche exhaustive où seul
  l'écart d'une carte devenue inutile est forcé (comparée à une recherche sans écart forcé).
  Sur les donnes 1 à 1000 : 508 gagnées, 108 perdues (prouvé), 384 indéterminées (limite de
  5 millions de positions ; 1,1 s au pire, 213 Mo au pic).

## Quarante & Huit

- Les règles de KPatience (Forty & Eight) : 104 cartes (2 jeux), 8 colonnes de 4 cartes
  visibles, 8 fondations de l'As au Roi dans la même couleur ; on descend dans la même
  couleur, une colonne vide reçoit n'importe quelle carte ; une suite de même couleur se
  déplace d'un bloc jusqu'à 2^(colonnes vides) cartes ; **Piocher** (Tab, ou un clic sur le
  talon) retourne une carte sur le rebut, qui se retourne en talon une seule fois ; toute
  carte qui peut sortir sort toute seule (dépôt automatique, comme KPatience) ;
- **outil de résolution** : portage de `FortyeightSolver` de KPatience sur le moteur
  `patsolve` (mêmes priorités et élagages), corrigé : l'original choisit mal la fondation
  d'un As (le second As d'une couleur ne sort jamais) et retourne le rebut en piochant dans
  le même coup. Sur les donnes 1 à 1000 : 929 gagnées, 0 perdue, 71 indéterminées (limite
  d'un million de positions ; 69 s pour les 1000, 0,6 s au pire, 155 Mo au pic).

## Treize à la douzaine

- Les règles de KPatience (Baker's Dozen) et ses variantes (menu Configuration) : Treize à
  la douzaine, Patience espagnole, Châteaux en Espagne, Solitaire portugais, personnalisée ;
  13 colonnes de 4 cartes, Rois au fond, pas de cellule ; suites au rang seul, dans la même
  couleur ou en couleurs alternées ; colonnes vides libres, aux Rois ou inutilisables ;
  cartes cachées (Châteaux en Espagne) ; dépôt automatique sans risque ;
- **outil de résolution** : KPatience utilise Freecell Solver ; c'est ici celui du FreeCell
  (13 colonnes, 0 cellule), qui sait maintenant garder des cartes cachées (il les connaît,
  comme Freecell Solver). Sur les donnes 1 à 100 : Treize à la douzaine 95 gagnées, 1 perdue,
  4 indéterminées ; Patience espagnole 100 / 0 / 0 ; Châteaux en Espagne 97 / 3 / 0 ;
  Solitaire portugais 94 / 1 / 5 (1 s au pire).

## Compilation

```sh
cargo build --release
./target/release/packarch-spider                    # jeu
./target/release/packarch-spider --deal 42 --suits 4 # Spider, donne n° 42, 4 couleurs
./target/release/packarch-spider --deal 42 --freecell 3 # FreeCell, donne n° 42
./target/release/packarch-spider --solve 2 1 100    # résout les donnes 1 à 100 (Spider, 2 couleurs)
./target/release/packarch-spider --deal 42 --klondike 3 # Klondike, donne n° 42, pioche 3
./target/release/packarch-spider --solve-freecell 3 1 100 # idem au FreeCell (variante 0 à 4)
./target/release/packarch-spider --solve-klondike 1 1 100 # idem au Klondike (pioche 1 ou 3)
./target/release/packarch-spider --deal 42 --clock  # horloge du grand-père, donne n° 42
./target/release/packarch-spider --solve-clock 1 100 # idem à l'horloge du grand-père
./target/release/packarch-spider --deal 42 --simon  # Simon le simple, donne n° 42
./target/release/packarch-spider --solve-simon 1 100 # idem à Simon le simple
./target/release/packarch-spider --deal 42 --golf   # Golf, donne n° 42
./target/release/packarch-spider --solve-golf 1 100  # idem au Golf
./target/release/packarch-spider --deal 42 --yukon  # Yukon, donne n° 42
./target/release/packarch-spider --solve-yukon 1 100 # idem au Yukon
./target/release/packarch-spider --deal 42 --grandfather # Grand-père, donne n° 42
./target/release/packarch-spider --solve-grandfather 1 100 # idem à Grand-père
./target/release/packarch-spider --deal 42 --mod3   # Mod3, donne n° 42
./target/release/packarch-spider --solve-mod3 1 100 # idem à Mod3
./target/release/packarch-spider --deal 42 --aces-up # les As, donne n° 42
./target/release/packarch-spider --solve-aces-up 1 100 # idem aux As
./target/release/packarch-spider --deal 42 --forty-eight # Quarante & Huit, donne n° 42
./target/release/packarch-spider --solve-forty-eight 1 100 # idem à Quarante & Huit
./target/release/packarch-spider --deal 42 --bakers 0 # Treize à la douzaine, donne n° 42 (variante 0 à 3)
./target/release/packarch-spider --solve-bakers 0 1 100 # idem à Treize à la douzaine
cargo test --release
```

Le binaire contient le jeu de cartes et le thème par défaut. Les autres sont cherchés dans
`<préfixe>/share/packarch-spider/{carddecks,themes}` (copie de `data/`), puis dans `data/`
des sources.

Installation (par exemple dans un PKGBUILD) :

```sh
install -Dm755 target/release/packarch-spider "$pkgdir"/usr/bin/packarch-spider
install -d "$pkgdir"/usr/share/packarch-spider
cp -r data/carddecks data/themes "$pkgdir"/usr/share/packarch-spider/
install -Dm644 data/org.packarch.Spider.desktop "$pkgdir"/usr/share/applications/org.packarch.Spider.desktop
for s in 16 22 24 32 48 64 128 256; do
    install -Dm644 data/icons/$s.png "$pkgdir"/usr/share/icons/hicolor/${s}x$s/apps/packarch-spider.png
done
```

## Fichiers

- `~/.config/packarch-spider/settings.json` : réglages ;
- `~/.config/packarch-spider/stats.json` : statistiques ;
- `~/.local/share/packarch-spider/state.json` : partie en cours, si « Se souvenir de l'état
  de la partie à sa fermeture » est coché.

## Code

- `src/cards.rs` : cartes, paquet et mélange (celui de KPatience, pour les numéros de donne) ;
- `src/board.rs` : ce qui est commun aux jeux (coups, partie vue par l'interface, variantes) ;
- `src/spider.rs` : règles du Spider, coups, annulation ;
- `src/spider_solver.rs` : outil de résolution du Spider, portage de `spidersolver2` de KPatience ;
- `src/freecell.rs` : règles du FreeCell et de ses variantes, dépôt automatique ;
- `src/freecell_solver.rs` : outil de résolution du FreeCell (aussi pour Treize à la douzaine) ;
- `src/klondike.rs` : règles du Klondike ;
- `src/patsolve.rs` : le moteur de recherche `patsolve` de KPatience, commun au Klondike, à l'horloge, au
  Yukon, à Grand-père, à Mod3 et à Quarante & Huit, et le stockage compact de leurs piles (`Piles`) ;
- `src/klondike_solver.rs` : outil de résolution du Klondike, portage de `KlondikeSolver` ;
- `src/clock.rs` : règles de l'horloge du grand-père ;
- `src/clock_solver.rs` : son outil de résolution, portage de `ClockSolver` ;
- `src/simon.rs` : règles de Simon le simple ;
- `src/simon_solver.rs` : son outil de résolution ;
- `src/golf.rs` : règles du Golf ;
- `src/golf_solver.rs` : son outil de résolution ;
- `src/yukon.rs` : règles du Yukon ;
- `src/yukon_solver.rs` : son outil de résolution, portage de `YukonSolver` ;
- `src/grandf.rs` : règles de Grand-père ;
- `src/grandf_solver.rs` : son outil de résolution, portage de `GrandfSolver` ;
- `src/mod3.rs` : règles de Mod3 ;
- `src/mod3_solver.rs` : son outil de résolution, portage de `Mod3Solver` ;
- `src/aces.rs` : règles des As ;
- `src/aces_solver.rs` : son outil de résolution ;
- `src/fortyeight.rs` : règles de Quarante & Huit ;
- `src/fortyeight_solver.rs` : son outil de résolution, portage corrigé de `FortyeightSolver` ;
- `src/bakers.rs` : règles et variantes de Treize à la douzaine ;
- `src/assets.rs` : jeux de cartes et thèmes SVG (rendus avec resvg) ;
- `src/storage.rs` : réglages, statistiques, partie en cours ;
- `src/app.rs` : interface.

## Licence et crédits

GPL-2.0-or-later (`COPYING`). D'après KPatience : © 1995 Paul Olav Tvete, © 2000 Stephan Kulow,
Josh Metzler (Spider), Stephan Kulow (outil de résolution du Spider), Rodolfo Borges, Stephan
Kulow et Parker Coates (FreeCell, Klondike, horloge du grand-père, Simon le simple, Golf, Yukon, Grand-père, Mod3, les As, Quarante & Huit, Treize à la douzaine), Tom Holroyd (patsolve) et les autres auteurs de KPatience. Jeux de cartes et thèmes :
libkdegames et KPatience (licences dans `data/carddecks/*`).
