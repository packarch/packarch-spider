//! Outil de résolution des As.
//!
//! Celui de KPatience (`IdiotSolver`) ne pose pas les cartes distribuées sur
//! les colonnes (`make_move` avance les piles sans y écrire la carte) : ses
//! résultats sont faux. Celui-ci est écrit pour ce portage : recherche en
//! profondeur sur toutes les suites de coups, positions déjà vues écartées
//! (empreinte de 128 bits). Si la recherche s'épuise sans dépasser sa
//! limite, la partie est vraiment perdue.
//!
//! Écarter une carte n'est pas toujours sans risque, contrairement à ce que
//! suppose KPatience (seul coup essayé quand il y en a un) : gardée, elle
//! peut battre plus tard une carte plus basse de sa couleur, quand celle qui
//! la bat aujourd'hui aura été recouverte (donne 34). L'écart n'est joué
//! d'office que si toutes les cartes plus basses de sa couleur sont déjà
//! écartées : elle ne peut plus servir. Sinon c'est un coup comme un autre
//! (essayé en premier). Le test `forced_discard_is_safe` compare avec une
//! recherche sans écart d'office.
//!
//! Cette recherche complète est lente sur certaines donnes : elle ne passe
//! qu'après une recherche rapide qui écarte tout d'office, comme KPatience
//! (ses solutions sont justes, seul son « perdu » ne prouve rien).

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::aces::{AWAY, Game, STORES, TALON};
use crate::board::{Move, Outcome};

/// Au-delà, la recherche abandonne (« impossible de déterminer »).
const MAX_POSITIONS: usize = 5_000_000;

/// Carte réduite : couleur × 16 + rang, l'As compté 14 (plus haut que tout).
type C = u8;

fn code(c: &crate::cards::Card) -> C {
    (c.suit as u8) << 4 | if c.rank == 1 { 14 } else { c.rank }
}

fn beats(top: C, card: C) -> bool {
    top >> 4 == card >> 4 && (top & 15) > (card & 15)
}

/// Les empreintes sont déjà bien mêlées : on les replie telles quelles.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8 | b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
    }
    fn write_u128(&mut self, n: u128) {
        self.0 = (n as u64) ^ (n >> 64) as u64;
    }
}

type Seen = HashSet<u128, BuildHasherDefault<KeyHasher>>;

/// Écarts joués d'office (seul coup essayé).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Discard {
    /// Tous : rapide, mais peut manquer une solution.
    All,
    /// Ceux des cartes qui ne servent plus : sans risque.
    Useless,
    /// Aucun : chaque écart est une branche (vérification).
    #[cfg(test)]
    None,
}

enum Stop {
    Aborted,
    Limit,
}

struct Search<'a> {
    /// Les colonnes, du fond vers le dessus.
    columns: [Vec<C>; STORES],
    /// Le talon, du fond vers le dessus (seules ses `talon` premières cartes
    /// y sont encore).
    stock: Vec<C>,
    talon: usize,
    away: usize,
    /// Les cartes écartées, un bit par carte (`C` < 64).
    gone: u64,
    seen: Seen,
    /// Les coups du chemin en cours.
    path: Vec<Move>,
    stop: &'a AtomicBool,
    /// Quels écarts sont joués d'office.
    discard: Discard,
    max_positions: usize,
}

impl Search<'_> {
    /// Empreinte de la position : deux hachages indépendants des colonnes et
    /// de la hauteur du talon.
    fn key(&self) -> u128 {
        let (mut a, mut b) = (0x243f_6a88_85a3_08d3u64 ^ self.talon as u64, 0x1319_8a2e_0370_7344u64);
        for col in &self.columns {
            for &c in col.iter().chain([&0xff]) {
                a = (a ^ c as u64).wrapping_mul(0x0000_0100_0000_01b3);
                b = (b.rotate_left(5) ^ c as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            }
        }
        // Mélange final, pour que les bits faibles dépendent de tout.
        a ^= a >> 33;
        a = a.wrapping_mul(0xff51_afd7_ed55_8ccd);
        a ^= a >> 33;
        (a as u128) << 64 | b as u128
    }

    fn top(&self, p: usize) -> Option<C> {
        self.columns[p].last().copied()
    }

    fn can_move_away(&self, p: usize) -> bool {
        self.top(p).is_some_and(|card| (0..STORES).any(|i| i != p && self.top(i).is_some_and(|t| beats(t, card))))
    }

    /// La carte du dessus de `p` ne servira plus : toutes les cartes plus
    /// basses de sa couleur sont écartées, ou le talon est vide (plus rien ne
    /// recouvrira celle qui la bat, qui reste visible tant qu'une plus haute
    /// ne l'écarte pas, et celle-là bat tout ce qu'elle battait).
    fn useless(&self, p: usize) -> bool {
        self.talon == 0
            || self.top(p).is_some_and(|card| {
                let lower = ((1u64 << (card & 15)) - (1 << 2)) << (card & 0x30);
                self.gone & lower == lower
            })
    }

    fn try_move(&mut self, from: usize, to: usize) -> Result<bool, Stop> {
        let card = self.columns[from].pop().expect("colonne");
        let index = self.columns[from].len();
        if to == AWAY {
            self.away += 1;
            self.gone |= 1 << card;
        } else {
            self.columns[to].push(card);
        }
        self.path.push(Move::Cards { from, index, to });
        let won = self.dfs()?;
        if !won {
            self.path.pop();
            if to == AWAY {
                self.away -= 1;
                self.gone &= !(1 << card);
            } else {
                self.columns[to].pop();
            }
            self.columns[from].push(card);
        }
        Ok(won)
    }

    /// Vrai si la position se gagne ; le chemin gagnant est alors dans `path`.
    fn dfs(&mut self) -> Result<bool, Stop> {
        // Gagné (`Idiot::isGameWon`) : les 4 cartes restantes sont les As
        // (rien ne les écarte), il faut encore qu'ils soient sortis du talon
        // et seuls sur leur colonne.
        if self.away == 48 && self.talon == 0 && self.columns.iter().all(|c| c.len() == 1) {
            return Ok(true);
        }
        // Écarter une carte qui ne sert plus : sans risque, seul coup essayé.
        let forced = |p: usize| match self.discard {
            Discard::All => true,
            Discard::Useless => self.useless(p),
            #[cfg(test)]
            Discard::None => false,
        };
        if let Some(p) = (0..STORES).find(|&p| self.can_move_away(p) && forced(p)) {
            return self.try_move(p, AWAY);
        }
        if !self.seen.insert(self.key()) {
            return Ok(false);
        }
        if self.seen.len().is_multiple_of(4096) {
            if self.stop.load(Ordering::Relaxed) {
                return Err(Stop::Aborted);
            }
            if self.seen.len() > self.max_positions {
                return Err(Stop::Limit);
            }
        }
        for p in 0..STORES {
            if self.can_move_away(p) && self.try_move(p, AWAY)? {
                return Ok(true);
            }
        }
        // Une carte sur une colonne vide (chaque colonne vide compte : la
        // rangée suivante tombe sur chacune).
        for to in 0..STORES {
            if !self.columns[to].is_empty() {
                continue;
            }
            for from in 0..STORES {
                if from != to && !self.columns[from].is_empty() && self.try_move(from, to)? {
                    return Ok(true);
                }
            }
        }
        if self.talon >= STORES {
            for i in 0..STORES {
                let c = self.stock[self.talon - 1 - i];
                self.columns[i].push(c);
            }
            self.talon -= STORES;
            self.path.push(Move::Deal);
            if self.dfs()? {
                return Ok(true);
            }
            self.path.pop();
            self.talon += STORES;
            for col in self.columns.iter_mut() {
                col.pop();
            }
        }
        Ok(false)
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    match search(game, stop, Discard::All, MAX_POSITIONS) {
        won @ (Outcome::Winnable(_) | Outcome::Aborted) => won,
        _ => search(game, stop, Discard::Useless, MAX_POSITIONS),
    }
}

fn search(game: &Game, stop: &AtomicBool, discard: Discard, max_positions: usize) -> Outcome {
    let columns: [Vec<C>; STORES] = std::array::from_fn(|i| game.piles[i].iter().map(code).collect());
    let stock: Vec<C> = game.piles[TALON].iter().map(code).collect();
    let mut search = Search {
        columns,
        talon: stock.len(),
        stock,
        away: game.piles[AWAY].len(),
        gone: game.piles[AWAY].iter().fold(0, |m, c| m | 1 << code(c)),
        seen: Seen::default(),
        path: Vec::new(),
        stop,
        discard,
        max_positions,
    };
    match search.dfs() {
        Ok(true) => Outcome::Winnable(search.path),
        Ok(false) => Outcome::NotWinnable,
        Err(Stop::Aborted) => Outcome::Aborted,
        Err(Stop::Limit) => Outcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in 1..=30 {
            let g = Game::new(deal);
            match solve(&g, &stop) {
                Outcome::Winnable(moves) => {
                    let mut end = g.clone();
                    for (k, &m) in moves.iter().enumerate() {
                        assert!(end.play(m), "donne {deal}, coup {k} illégal : {m:?}");
                    }
                    assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                    solved += 1;
                }
                Outcome::NotWinnable | Outcome::Unknown => {}
                Outcome::Aborted => panic!("donne {deal} : arrêtée"),
            }
        }
        assert!(solved >= 10, "{solved} donnes résolues sur 30");
    }

    /// Une position gagnée d'avance, et une perdue : un seul coup possible
    /// au départ, puis plus rien.
    #[test]
    fn won_and_lost_positions() {
        use crate::cards::{Card, Suit};
        let stop = AtomicBool::new(false);
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        let card = |rank, suit| Card { id: 0, rank, suit, face_up: true };
        for (i, s) in Suit::ALL.into_iter().enumerate() {
            g.piles[i].push(card(1, s));
        }
        g.piles[AWAY] = vec![card(2, Suit::Clubs); 47];
        // Le 5 de trèfle sur l'As de carreau : l'As de trèfle l'écarte.
        g.piles[1].push(card(5, Suit::Clubs));
        assert_eq!(solve(&g, &stop), Outcome::Winnable(vec![Move::Cards { from: 1, index: 1, to: AWAY }]));
        // Chaque As sous une carte que seul lui battrait, pas de colonne
        // vide, talon vide : perdu.
        g.piles[0] = vec![card(1, Suit::Clubs), card(9, Suit::Hearts)];
        g.piles[1] = vec![card(1, Suit::Diamonds), card(5, Suit::Clubs)];
        g.piles[2] = vec![card(1, Suit::Hearts), card(2, Suit::Diamonds)];
        g.piles[AWAY].truncate(44);
        assert_eq!(solve(&g, &stop), Outcome::NotWinnable);
    }

    /// La donne 34 ne se gagne qu'en gardant une carte qu'on pourrait
    /// écarter : tout écarter d'office (KPatience) la croit perdue.
    #[test]
    fn deal_34_needs_a_kept_card() {
        let stop = AtomicBool::new(false);
        let g = Game::new(34);
        assert_eq!(search(&g, &stop, Discard::All, MAX_POSITIONS), Outcome::NotWinnable);
        let Outcome::Winnable(moves) = solve(&g, &stop) else { panic!("donne 34 gagnable") };
        let mut end = g.clone();
        assert!(moves.iter().all(|&m| end.play(m)) && end.is_won());
    }

    /// Vérification : écarter d'office est sans risque. Même recherche,
    /// mais chaque écart possible est une branche ; elle doit conclure
    /// pareil quand elle tranche (sur une donne perdue, elle peut demander
    /// des centaines de millions de positions : limitée à 10 millions).
    /// `cargo test --release -- --ignored`.
    #[test]
    #[ignore]
    fn forced_discard_is_safe() {
        let stop = AtomicBool::new(false);
        let mut settled = 0;
        for deal in 1..=100 {
            let g = Game::new(deal);
            let forced = search(&g, &stop, Discard::Useless, MAX_POSITIONS);
            match search(&g, &stop, Discard::None, 10_000_000) {
                Outcome::Unknown => {}
                free => {
                    assert!(!matches!(forced, Outcome::Unknown | Outcome::Aborted), "donne {deal} : {forced:?}");
                    assert_eq!(matches!(forced, Outcome::Winnable(_)), matches!(free, Outcome::Winnable(_)), "donne {deal}");
                    settled += 1;
                }
            }
        }
        assert!(settled >= 40, "{settled} donnes tranchées sur 100");
        eprintln!("{settled} donnes tranchées sur 100 sans écart forcé, mêmes conclusions");
    }
}
