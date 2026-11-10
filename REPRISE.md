# Où en est le portage (notes de reprise)

Dernière mise à jour : 10 novembre 2026 (soir).

Projet : `packarch-spider`, portage en Rust (egui) des jeux de KPatience. Les sources C++
de référence sont dans `/mnt/ssd/dev/kpat` (fork Packarch Spider, C++/Qt ; aussi au tag `v1.0.1`
de ce dépôt).

Publié le 10 novembre 2026 dans le dépôt `packarch/packarch-spider` (version 2.0.0, puis 2.1.0 avec
les As et Quarante & Huit), à la place
de la version C++ (jusqu'à `v1.0.1`).

## Jeux portés

| Jeu | Règles | Solveur | Résultat du solveur |
|---|---|---|---|
| Spider (1 à 4 couleurs) | `spider.rs` | `spider_solver.rs`, portage de `spidersolver2`, **parallélisé** (rayon) | ≈ 0,1 s/donne, résultats identiques à l'original |
| FreeCell + Baker's Game, Eight Off, Forecell, Seahaven, personnalisé | `freecell.rs` | `freecell_solver.rs`, **maison** (KPat utilise la lib C Freecell Solver) | 1000/1000 donnes FreeCell, ≈ 2 ms |
| Klondike (pioche 1 ou 3) | `klondike.rs` | `klondike_solver.rs`, portage de `KlondikeSolver` sur `patsolve.rs`, état compact de 70 octets | 87,7 % / 78,2 % gagnées, 10,0 % / 9,9 % indéterminées (limite 1 million de positions, 0,6 s au pire) |
| L'horloge du grand-père | `clock.rs` | `clock_solver.rs`, portage de `ClockSolver` sur `patsolve.rs` | 97/100, ≈ 2 ms |
| Simon le simple | `simon.rs` | `simon_solver.rs`, **maison** (KPat utilise Freecell Solver), positions en attente compactées | 943/1000 gagnées, 56 perdues (prouvé), 1 indéterminée ; 6,4 s et 54 Mo au pic pour les 1000 |
| Golf | `golf.rs` | `golf_solver.rs`, **maison** (KPat utilise black-hole-solver) : recherche exhaustive, position en 30 bits | 463/1000 gagnées, 537 perdues (prouvé), 0 indéterminée ; ≈ 24 ms/donne, 0,16 s au pire ; identique à KPat sur les 885 donnes qu'il tranche |
| Yukon | `yukon.rs` | `yukon_solver.rs`, portage de `YukonSolver` sur `patsolve.rs` (état compact `Piles<7>`) | 727/1000 gagnées, 47 perdues, 226 indéterminées (limite 1 million de positions) ; ≈ 0,15 s/donne, 1 s au pire ; KPat 714/45/241, jamais contredit |
| Grand-père | `grandf.rs` | `grandf_solver.rs`, portage de `GrandfSolver` sur `patsolve.rs` (redistribution = coup, historique haché dans la clé comme KPat) | 997/1000 gagnées, 2 perdues, 1 indéterminée ; 5 s pour les 1000 ; **identique à KPat sur les 1000** |
| Les as (Aces Up) | `aces.rs` | `aces_solver.rs`, **maison** (`IdiotSolver` de KPat est faux : sa distribution ne pose pas les cartes, et écarter d'office n'est pas sûr, donne 34). Deux passes : tout écarter d'office (rapide, solutions justes), puis recherche exhaustive où seul l'écart d'une carte inutile est forcé (plus basses de sa couleur écartées, ou talon vide) ; vérifié contre une recherche sans écart forcé (`forced_discard_is_safe`, 45 donnes tranchées sur 100, 95 s) | 508/1000 gagnées, 108 perdues (prouvé), 384 indéterminées (limite 5 millions) ; 254 s pour les 1000, 1,1 s au pire, 213 Mo au pic. Piste : un meilleur critère d'écart sans risque réduirait les indéterminées |
| Quarante & Huit (Forty & Eight) | `fortyeight.rs` | `fortyeight_solver.rs`, portage de `FortyeightSolver` sur `patsolve.rs` (`Piles<10, 104>`), **corrigé** : sorties (l'original ne sort jamais le second As d'une couleur), rebut retourné en un coup distinct de la pioche | 929/1000 gagnées, 0 perdue, 71 indéterminées (limite 1 million) ; 69 s pour les 1000, 0,6 s au pire, 155 Mo au pic |
| Treize à la douzaine (Baker's Dozen) + 3 variantes et personnalisée | `bakers.rs` | celui du FreeCell (`freecell_solver::solve_piles`, 13 colonnes, 0 cellule ; KPat : Freecell Solver), qui garde maintenant des cartes cachées (`Code::hidden`, `HIDDEN`) | donnes 1 à 100 : 95/1/4, Patience espagnole 100/0/0, Châteaux en Espagne 97/3/0, Solitaire portugais 94/1/5 ; 1 s au pire |
| Château (Castle) + 5 variantes et personnalisée | `castle.rs` | celui du FreeCell (`solve_piles`, réserves = cellules ; KPat : Freecell Solver) | donnes 1 à 100 : Château assiégé 71/28/1, Citadelle 87/13/0, Rois exilés 77/15/8, Rues et allées 47/45/8, Engin de siège 97/2/1, Forteresse 95/5/0 ; 0,6 s au pire |
| Normandie (Normandy) | `normandy.rs` | `normandy_solver.rs`, portage de `NormandySolver` sur `patsolve.rs` (`Piles<9, 104>`, priorités `params`, Raymond à 8 fondations) ; grappe = fondations (KPat : 0) : 81/100 au lieu de 73 | 868/1000 gagnées, 0 perdue, 132 indéterminées (limite 1 million) ; 85 s pour les 1000, 1,5 s au pire, 207 Mo au pic |
| Mod3 | `mod3.rs` | `mod3_solver.rs`, portage de `Mod3Solver` sur `patsolve.rs` (`Piles<34, 104>`) | 747/1000 gagnées, 19 perdues, 234 indéterminées ; ≈ 0,5 s/donne, 2,6 s au pire, **245 Mo au pic** ; KPat 541/18/441, jamais contredit |

Aussi fait :
- écran de choix du jeu comme `GameSelectionScene` de KPatience (Nouvelle partie… Ctrl+Maj+N),
  au lancement s'il n'y a pas de partie gardée ;
- un clic joue la carte au Golf et aux As (`tryAutomaticMove`) ;
- dépôt automatique (FreeCell, Klondike, Simon, Golf, Yukon, Grand-père, Mod3, Quarante & Huit), bouton Déposer (P), Piocher
  (Tab, Klondike, Golf et Quarante & Huit), Redistribuer (R, Grand-père), Distribuer (Entrée, Spider, Mod3 et les As) ;
- noms complets dans le titre et les statistiques (« FreeCell (Baker's Game) »…).

## Chantier en cours (à lire en premier après une coupure)

Demande du 10 novembre 2026 : porter tous les jeux de KPatience qui manquent. Ordre et état
(mis à jour à chaque étape ; un jeu n'est « fait » que testé, documenté et commité) :

1. Treize à la douzaine (Baker's Dozen et variantes : `bakersdozen.cpp`,
   `patsolve/bakersdozensolver.cpp`) — **fait** (commit « Treize à la douzaine »)
2. Château (Castle et variantes : `castle.cpp`, `patsolve/castlesolver.cpp`) — **fait** (commit « Château »)
3. Normandie (Normandy : `normandy.cpp`, `patsolve/normandysolver.cpp`) — **fait** (commit « Normandie »)
4. Bizilizi (et variantes : `bizilizi.cpp`, `patsolve/bizilizisolver.cpp`) — à faire

Ensuite : version 2.2.0 (Cargo.toml, tag), PKGBUILD (`myrepo-pkgs/update.sh --apps
packarch-spider`), `build.sh packarch-spider`, `myrepo/update.sh`, vérifier GitHub Pages
(`gh run list -R packarch/myrepo`), installer.

Étape en cours : Bizilizi (`bizilizi.cpp`, `patsolve/bizilizisolver.cpp`) : lecture des sources.

## Architecture : où brancher un nouveau jeu

- `board.rs` : `Move`, `AnyGame` (enum `untagged` : un jeu sans champ distinctif doit en avoir
  un, comme `clock: bool`, `simon: bool`), `GameKind`, `Variant`, `solve()`.
- `app.rs` : disposition par piles (`Layout::<jeu>`, coordonnées de `initialize()` de
  KPatience, x en largeurs et y en hauteurs de carte), liste `GAMES` de l'écran de choix
  (aperçu copié de `kpat/previews/<id>.png` dans `data/previews/`), menus, règles
  (`RULES_<JEU>`).
- `storage.rs` : réglages et statistiques par variante.
- `main.rs` : `--deal N --<jeu>` et `--solve-<jeu> DÉBUT FIN` pour tester.
- `patsolve.rs` : moteur `patsolve` générique (trait `Position`) pour les solveurs de
  KPatience qui en dérivent (Klondike, Horloge, Yukon, Grand-père, Mod3, Quarante & Huit,
  Normandie ; à réutiliser pour Bizilizi), et `Piles<N, C>`, le
  stockage compact (sans allocation) de N piles et C cartes (52 par défaut).

## Prochaines étapes possibles

1. Suite de l'étude d'optimisation des solveurs (faits le 9 novembre 2026 : positions de
   Simon compactées, 265 → 54 Mo et 14,4 → 6,4 s ; état du Klondike en tableau fixe, puis
   limite relevée à 1 million de positions) :
   - facultatif : « portefeuille » (plusieurs réglages en parallèle) pour FreeCell/Simon ;
   - Klondike : les ≈ 100 indéterminées restantes (sur 1000) demanderaient une autre méthode
     (élagage, heuristique) plutôt qu'une limite plus haute ;
   - Mod3 : 245 Mo au pic avec 1 million de positions (état de 138 octets, talon compris) ;
     compacter le talon (son ordre est fixe : un compteur suffit) le réduirait nettement.
2. Autres jeux de KPatience pas encore portés (voir « Chantier en cours ») : Normandie, Château (Castle), Treize à la
   douzaine (Baker's Dozen), Bizilizi.

## Comment vérifier

```sh
cargo build --release && cargo test --release          # 90 tests
./target/release/packarch-spider --solve 2 1 30       # Spider
./target/release/packarch-spider --solve-freecell 3 1 100
./target/release/packarch-spider --solve-klondike 1 1 100
./target/release/packarch-spider --solve-clock 1 100
./target/release/packarch-spider --solve-simon 1 100
./target/release/packarch-spider --solve-golf 1 100
./target/release/packarch-spider --solve-yukon 1 100
./target/release/packarch-spider --solve-grandfather 1 100
./target/release/packarch-spider --solve-mod3 1 100
./target/release/packarch-spider --solve-aces-up 1 1000
./target/release/packarch-spider --solve-forty-eight 1 100
cargo test --release -- --ignored                       # vérifications exhaustives (≈ 4 min)
```

Tests de l'interface sans toucher aux réglages de l'utilisateur : lancer avec
`XDG_CONFIG_HOME` et `XDG_DATA_HOME` pointant vers un dossier temporaire, capturer la
fenêtre (`import -window`), sans clic simulé ; ne fermer que sa propre instance.
