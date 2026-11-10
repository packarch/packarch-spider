//! Outil de résolution du Golf.
//!
//! KPatience confie ce jeu à une bibliothèque externe (black-hole-solver) ;
//! celui-ci est écrit pour ce portage. Une position se réduit à la hauteur
//! des 7 colonnes, au nombre de cartes du talon et au rang du dessus du rebut
//! (la couleur ne compte pas) : 30 bits. Recherche en profondeur, positions
//! déjà vues écartées ; tous les coups sont essayés, donc si la recherche
//! s'épuise sans dépasser sa limite, la partie est vraiment perdue.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::board::{Move, Outcome};
use crate::golf::{DEPTH, Game, STORES, TALON, WASTE, fits};

/// Au-delà, la recherche abandonne (« impossible de déterminer »).
const MAX_POSITIONS: usize = 5_000_000;

#[derive(Clone, Copy)]
struct State {
    heights: [u8; STORES],
    talon: u8,
    top: u8,
}

impl State {
    fn key(&self) -> u32 {
        let mut k = 0u32;
        for &h in &self.heights {
            k = (k << 3) | h as u32;
        }
        (k << 9) | (self.talon as u32) << 4 | self.top as u32
    }
}

/// Les clés tiennent en 30 bits : une multiplication suffit à les répartir
/// (bien plus rapide que le hachage par défaut).
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
    fn write_u32(&mut self, n: u32) {
        // Les bits forts du produit, bien mêlés, replis sur les bits faibles
        // (ceux qui choisissent la case).
        let x = (n as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.0 = x ^ (x >> 32);
    }
}

type Seen = HashSet<u32, BuildHasherDefault<KeyHasher>>;

enum Stop {
    Aborted,
    Limit,
}

struct Search<'a> {
    /// Rangs des colonnes, du fond vers le dessus.
    columns: [[u8; DEPTH]; STORES],
    /// Rangs du talon, du fond vers le dessus.
    talon: Vec<u8>,
    seen: Seen,
    /// Les coups du chemin en cours.
    path: Vec<Move>,
    stop: &'a AtomicBool,
}

impl Search<'_> {
    /// Vrai si la position se gagne ; le chemin gagnant est alors dans `path`.
    fn dfs(&mut self, s: State) -> Result<bool, Stop> {
        if s.heights.iter().all(|&h| h == 0) {
            // Le reste du talon se retourne (dépôt automatique).
            self.path.extend(std::iter::repeat_n(Move::Deal, s.talon as usize));
            return Ok(true);
        }
        if !self.seen.insert(s.key()) {
            return Ok(false);
        }
        if self.seen.len().is_multiple_of(4096) {
            if self.stop.load(Ordering::Relaxed) {
                return Err(Stop::Aborted);
            }
            if self.seen.len() > MAX_POSITIONS {
                return Err(Stop::Limit);
            }
        }
        // Les colonnes les plus hautes d'abord : ce sont elles qui bloquent.
        let mut order: [usize; STORES] = std::array::from_fn(|i| i);
        order.sort_by_key(|&i| std::cmp::Reverse(s.heights[i]));
        for i in order {
            let h = s.heights[i] as usize;
            if h == 0 {
                continue;
            }
            let card = self.columns[i][h - 1];
            if !fits(card, s.top) {
                continue;
            }
            let mut next = s;
            next.heights[i] -= 1;
            next.top = card;
            self.path.push(Move::Cards { from: i, index: h - 1, to: WASTE });
            if self.dfs(next)? {
                return Ok(true);
            }
            self.path.pop();
        }
        if s.talon > 0 {
            let next = State { talon: s.talon - 1, top: self.talon[s.talon as usize - 1], ..s };
            self.path.push(Move::Deal);
            if self.dfs(next)? {
                return Ok(true);
            }
            self.path.pop();
        }
        Ok(false)
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    let mut columns = [[0; DEPTH]; STORES];
    let mut heights = [0; STORES];
    for (i, col) in game.piles[..STORES].iter().enumerate() {
        for (j, c) in col.iter().enumerate() {
            columns[i][j] = c.rank;
        }
        heights[i] = col.len() as u8;
    }
    let talon: Vec<u8> = game.piles[TALON].iter().map(|c| c.rank).collect();
    let Some(top) = game.piles[WASTE].last().map(|c| c.rank) else { return Outcome::NotWinnable };
    let start = State { heights, talon: talon.len() as u8, top };
    let mut search = Search { columns, talon, seen: Seen::default(), path: Vec::new(), stop };
    match search.dfs(start) {
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
                Outcome::NotWinnable => {}
                other => panic!("donne {deal} : {other:?}"),
            }
        }
        assert!(solved >= 5, "{solved} donnes résolues sur 30");
    }

    /// Les donnes 1 à 12, comme black-hole-solver dans KPatience (son test
    /// `golf_solver_wins.cpp` vérifie la donne 2).
    #[test]
    fn first_deals_agree_with_kpatience() {
        let stop = AtomicBool::new(false);
        let won: Vec<u32> = (1..=12).filter(|&d| matches!(solve(&Game::new(d), &stop), Outcome::Winnable(_))).collect();
        assert_eq!(won, vec![2, 4, 5, 10, 12]);
    }

    /// Vérification : recherche exhaustive naïve sur les vraies cartes,
    /// sans réduire la position. `cargo test --release -- --ignored`.
    #[test]
    #[ignore]
    fn results_agree_with_brute_force() {
        use crate::cards::Card;
        fn brute(piles: &mut Vec<Vec<Card>>, seen: &mut HashSet<Vec<usize>>) -> bool {
            if piles[..STORES].iter().all(Vec::is_empty) {
                return true;
            }
            let key: Vec<usize> = piles.iter().map(Vec::len).chain(piles[WASTE].last().map(|c| c.id as usize)).collect();
            if !seen.insert(key) {
                return false;
            }
            let top = piles[WASTE].last().expect("rebut").rank;
            for from in (0..STORES).chain([TALON]) {
                let Some(&card) = piles[from].last() else { continue };
                if from != TALON && !fits(card.rank, top) {
                    continue;
                }
                piles[from].pop();
                piles[WASTE].push(card);
                let won = brute(piles, seen);
                piles[WASTE].pop();
                piles[from].push(card);
                if won {
                    return true;
                }
            }
            false
        }
        let stop = AtomicBool::new(false);
        for deal in 1..=300 {
            let g = Game::new(deal);
            let ours = matches!(solve(&g, &stop), Outcome::Winnable(_));
            assert_eq!(ours, brute(&mut g.piles.clone(), &mut HashSet::new()), "donne {deal}");
        }
    }
}
