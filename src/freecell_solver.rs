//! Outil de résolution du FreeCell (et de ses variantes).
//!
//! KPatience confie le FreeCell à une bibliothèque externe (Freecell Solver,
//! en C) ; celui-ci est écrit pour ce portage, dans l'esprit de `patsolve` :
//! recherche « meilleur d'abord » (les positions les plus prometteuses sont
//! examinées en premier), positions déjà vues écartées (colonnes et cellules
//! sont interchangeables, elles sont triées pour les comparer), sorties sans
//! risque vers les fondations jouées d'office (la même règle que le dépôt
//! automatique du jeu, pour que la solution soit rejouable telle quelle).
//!
//! Tous les coups possibles sont essayés (aux symétries près) : si la
//! recherche s'épuise sans dépasser sa limite, la partie est vraiment perdue.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::board::{Move, Outcome};
use crate::freecell::{Code, EmptyFill, Game, PileKind, Rules, rank, suit};

/// Au-delà, la recherche abandonne (« impossible de déterminer »).
const MAX_POSITIONS: usize = 400_000;

#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    from: u8,
    index: u8,
    to: u8,
}

const ROOT: u32 = u32::MAX;

/// Les clés des positions sont déjà des hachages : inutile de les rehacher.
#[derive(Default)]
struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | b as u64;
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

type Seen = HashSet<u64, BuildHasherDefault<IdentityHasher>>;

struct Entry {
    score: Reverse<(i32, u32)>,
    node: u32,
    depth: i32,
    state: Box<[u8]>,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score.cmp(&other.score)
    }
}

struct Search<'a> {
    rules: Rules,
    nodes: Vec<Node>,
    seen: Seen,
    heap: BinaryHeap<Entry>,
    counter: u32,
    stop: &'a AtomicBool,
}

/// État compact : les piles bout à bout, séparées par 0 (aucune carte ne
/// vaut 0). Les fondations ne gardent que leur carte du dessus.
fn pack(piles: &[Vec<u8>]) -> Box<[u8]> {
    let mut v = Vec::with_capacity(piles.iter().map(|p| p.len() + 1).sum());
    for p in piles {
        v.extend_from_slice(p);
        v.push(0);
    }
    v.into_boxed_slice()
}

fn unpack(state: &[u8], piles: &mut [Vec<u8>]) {
    let mut i = 0;
    for p in piles.iter_mut() {
        p.clear();
        while state[i] != 0 {
            p.push(state[i]);
            i += 1;
        }
        i += 1;
    }
}

fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

impl<'a> Search<'a> {
    /// Clé de la position, aux symétries près : colonnes triées, cellules
    /// triées, fondations réduites à la hauteur de chaque couleur.
    fn key(&self, piles: &[Vec<u8>]) -> u64 {
        let r = &self.rules;
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut cols: Vec<&Vec<u8>> = piles[..r.stacks()].iter().collect();
        cols.sort_unstable();
        for c in cols {
            fnv(&mut h, c);
            fnv(&mut h, &[0]);
        }
        let mut cells: Vec<u8> = piles[r.first_cell()..r.first_foundation()].iter().map(|p| p.last().copied().unwrap_or(0)).collect();
        cells.sort_unstable();
        fnv(&mut h, &cells);
        let mut found: Vec<u8> = piles[r.first_foundation()..].iter().map(|p| p.last().copied().unwrap_or(0)).collect();
        found.sort_unstable();
        fnv(&mut h, &found);
        h
    }

    fn out(&self, piles: &[Vec<u8>]) -> usize {
        piles[self.rules.first_foundation()..].iter().map(|p| p.last().map_or(0, |&c| rank(c) as usize)).sum()
    }

    /// Plus c'est petit, mieux c'est : cartes restantes, cartes posées sur
    /// une carte plus basse (qu'il faudra déplacer), cellules occupées,
    /// colonnes vides. La profondeur s'y ajoute (une unité par coup) : la
    /// recherche préfère les chemins courts, ce qui la rend bien plus sûre
    /// (réglage mesuré sur les donnes 1 à 300).
    fn score(&self, piles: &[Vec<u8>]) -> i32 {
        let r = &self.rules;
        let left = (r.total_cards() - self.out(piles)) as i32;
        let mut blocked = 0;
        let mut empty = 0;
        for col in &piles[..r.stacks()] {
            if col.is_empty() {
                empty += 1;
                continue;
            }
            let mut lowest = 14;
            for &c in col {
                if rank(c) > lowest {
                    blocked += 1;
                }
                lowest = lowest.min(rank(c));
            }
        }
        let cells = piles[r.first_cell()..r.first_foundation()].iter().filter(|p| !p.is_empty()).count() as i32;
        let empty_bonus = if r.opts.empty_fill == EmptyFill::None { 0 } else { empty };
        2 * (left + blocked + cells - empty_bonus)
    }

    fn apply(&self, piles: &mut [Vec<u8>], from: usize, index: usize, to: usize) {
        let moving = piles[from].split_off(index);
        if self.rules.kind(to) == PileKind::Foundation {
            piles[to].clear();
            piles[to].push(*moving.last().expect("une carte"));
        } else {
            piles[to].extend(moving);
        }
    }

    fn push_node(&mut self, parent: u32, from: usize, index: usize, to: usize) -> u32 {
        self.nodes.push(Node { parent, from: from as u8, index: index as u8, to: to as u8 });
        (self.nodes.len() - 1) as u32
    }

    /// Joue les sorties sans risque, chacune dans un nœud à part.
    fn automoves(&mut self, piles: &mut [Vec<u8>], mut node: u32) -> u32 {
        while let Some(Move::Cards { from, index, to }) = self.rules.autodrop(piles) {
            self.apply(piles, from, index, to);
            node = self.push_node(node, from, index, to);
        }
        node
    }

    /// Les coups depuis la position (hors sorties sans risque, déjà jouées).
    fn moves(&self, piles: &[Vec<u8>]) -> Vec<(usize, usize, usize)> {
        let r = &self.rules;
        let mut out = Vec::new();
        let first_empty_col = (0..r.stacks()).find(|&c| piles[c].is_empty());
        let first_empty_cell = (r.first_cell()..r.first_foundation()).find(|&c| piles[c].is_empty());
        for from in 0..r.first_foundation() {
            let pile = &piles[from];
            let Some(&top) = pile.last() else { continue };
            // Vers une fondation.
            if let Some(to) = r.foundation_for(piles, top) {
                out.push((from, pile.len() - 1, to));
            }
            let is_cell = r.kind(from) == PileKind::Cell;
            // Le début de la suite du dessus de la colonne.
            let mut start = pile.len() - 1;
            if !is_cell {
                while start > 0 && r.builds_on(pile[start], pile[start - 1]) {
                    start -= 1;
                }
            }
            for to in 0..r.stacks() {
                if to == from {
                    continue;
                }
                if piles[to].is_empty() {
                    if Some(to) != first_empty_col {
                        continue;
                    }
                    for index in start..pile.len() {
                        // Toute une colonne vers une colonne vide : inutile.
                        if (is_cell || index > 0) && r.can_drop(piles, from, index, to) {
                            out.push((from, index, to));
                        }
                    }
                } else {
                    let t = *piles[to].last().expect("non vide");
                    if let Some(index) = (start..pile.len()).find(|&i| r.builds_on(pile[i], t)) {
                        if r.can_drop(piles, from, index, to) {
                            out.push((from, index, to));
                        }
                    }
                }
            }
            // Vers une cellule libre.
            if !is_cell {
                if let Some(cell) = first_empty_cell {
                    out.push((from, pile.len() - 1, cell));
                }
            }
        }
        out
    }

    fn path(&self, mut node: u32) -> Vec<Move> {
        let mut moves = Vec::new();
        while node != ROOT {
            let n = self.nodes[node as usize];
            moves.push(Move::Cards { from: n.from as usize, index: n.index as usize, to: n.to as usize });
            node = n.parent;
        }
        moves.reverse();
        moves
    }

    fn run(&mut self, start: Vec<Vec<u8>>) -> Outcome {
        let mut piles = start;
        let total = self.rules.total_cards();
        let node = self.automoves(&mut piles, ROOT);
        if self.out(&piles) == total {
            return Outcome::Winnable(self.path(node));
        }
        self.seen.insert(self.key(&piles));
        let score = self.score(&piles);
        self.heap.push(Entry { score: Reverse((score, 0)), node, depth: 0, state: pack(&piles) });

        let mut expanded = 0usize;
        let mut work = piles.clone();
        while let Some(entry) = self.heap.pop() {
            expanded += 1;
            if expanded % 1024 == 0 && self.stop.load(Ordering::Relaxed) {
                return Outcome::Aborted;
            }
            if self.seen.len() > MAX_POSITIONS {
                return Outcome::Unknown;
            }
            unpack(&entry.state, &mut piles);
            for (from, index, to) in self.moves(&piles) {
                work.clone_from_slice(&piles);
                self.apply(&mut work, from, index, to);
                let mut node = self.push_node(entry.node, from, index, to);
                node = self.automoves(&mut work, node);
                if !self.seen.insert(self.key(&work)) {
                    continue;
                }
                if self.out(&work) == total {
                    return Outcome::Winnable(self.path(node));
                }
                self.counter += 1;
                let depth = entry.depth + 1;
                let score = self.score(&work) + depth;
                self.heap.push(Entry { score: Reverse((score, self.counter)), node, depth, state: pack(&work) });
            }
        }
        Outcome::NotWinnable
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    let rules = game.rules();
    let piles: Vec<Vec<u8>> = game
        .piles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if rules.kind(i) == PileKind::Foundation {
                p.last().map(|c| c.code()).into_iter().collect()
            } else {
                p.iter().map(|c| c.code()).collect()
            }
        })
        .collect();
    debug_assert!(piles.iter().flatten().all(|&c| (1..=13).contains(&rank(c)) && suit(c) < 4));
    let mut search = Search { rules, nodes: Vec::new(), seen: Seen::default(), heap: BinaryHeap::new(), counter: 0, stop };
    search.run(piles)
}

/// Joue la solution sur une copie du jeu (tests, vérification).
#[cfg(test)]
pub fn replay(game: &Game, moves: &[Move]) -> Option<Game> {
    let mut g = game.clone();
    for &m in moves {
        if !g.play(m) {
            return None;
        }
    }
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::freecell::{Options, PRESETS};

    fn check(options: Options, deals: std::ops::RangeInclusive<u32>, min_solved: usize) {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in deals.clone() {
            let g = Game::new(options, deal);
            if let Outcome::Winnable(moves) = solve(&g, &stop) {
                let end = replay(&g, &moves).expect("solution jouable");
                assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                solved += 1;
            }
        }
        assert!(solved >= min_solved, "{solved} donnes résolues sur {}", deals.count());
    }

    #[test]
    fn freecell_deals_are_solved_and_replayable() {
        check(Options::default(), 1..=10, 10);
    }

    #[test]
    fn variants_are_solved_and_replayable() {
        for (_, o) in PRESETS {
            check(o, 1..=3, 1);
        }
        check(Options { decks: 2, stacks: 10, cells: 6, ..Options::default() }, 1..=2, 1);
    }

    #[test]
    fn hopeless_position_is_lost() {
        // Pas de cellule, pas de colonne vide utilisable, rien ne bouge.
        let o = Options { cells: 0, empty_fill: crate::freecell::EmptyFill::None, ..Options::default() };
        let stop = AtomicBool::new(false);
        let mut lost = 0;
        for deal in 1..=20 {
            if solve(&Game::new(o, deal), &stop) == Outcome::NotWinnable {
                lost += 1;
            }
        }
        assert!(lost > 0);
    }

    /// Vérification : recherche exhaustive naïve (une carte à la fois, tous
    /// les coups, positions exactes). `cargo test --release -- --ignored`.
    fn brute_force(g: &Game, limit: usize) -> Option<bool> {
        let r = g.rules();
        let start: Vec<Vec<u8>> = g.piles.iter().map(|p| p.iter().map(|c| c.code()).collect()).collect();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![start];
        while let Some(piles) = stack.pop() {
            if piles[r.first_foundation()..].iter().map(Vec::len).sum::<usize>() == r.total_cards() {
                return Some(true);
            }
            if !seen.insert(piles.clone()) {
                continue;
            }
            if seen.len() > limit {
                return None;
            }
            for from in 0..r.first_foundation() {
                let n = piles[from].len();
                if n == 0 {
                    continue;
                }
                for to in 0..r.piles() {
                    if r.is_legal(&piles, from, n - 1, to) {
                        let mut next = piles.clone();
                        let c = next[from].pop().unwrap();
                        next[to].push(c);
                        if !seen.contains(&next) {
                            stack.push(next);
                        }
                    }
                }
            }
        }
        Some(false)
    }

    #[test]
    #[ignore]
    fn lost_deals_agree_with_brute_force() {
        let stop = AtomicBool::new(false);
        let mut checked = 0;
        for (v, deals) in [(0usize, 150..=200u32), (2, 150..=200), (4, 150..=200)] {
            let o = PRESETS[v].1;
            for deal in deals {
                let g = Game::new(o, deal);
                if solve(&g, &stop) == Outcome::NotWinnable {
                    let b = brute_force(&g, 3_000_000);
                    eprintln!("variante {v} donne {deal} : exhaustif {b:?}");
                    assert_ne!(b, Some(true), "variante {v} donne {deal} : gagnable");
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }
}
