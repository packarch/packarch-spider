//! Outil de résolution de Simon le simple.
//!
//! KPatience confie ce jeu à une bibliothèque externe (Freecell Solver) ;
//! celui-ci est écrit pour ce portage, comme celui du FreeCell : recherche
//! « meilleur d'abord », positions déjà vues écartées (les colonnes sont
//! interchangeables : elles sont triées pour les comparer), suites complètes
//! montées d'office sur les fondations (la règle du dépôt automatique, pour
//! que la solution soit rejouable telle quelle).
//!
//! Tous les coups possibles sont essayés (aux symétries près) : si la
//! recherche s'épuise sans dépasser sa limite, la partie est vraiment perdue.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::board::{Move, Outcome};
use crate::freecell::{Code, rank, suit};
use crate::patsolve::{FNV_START, fnv};
use crate::simon::{self, FOUNDATION, Game, PILES, STORES};

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

/// Position compacte des positions en attente : chaque colonne suivie d'un
/// 0 (aucune carte n'a le code 0), puis un octet par fondation, le Roi de la
/// suite montée ou 0 (une fondation pleine est toujours Roi → As d'une couleur).
fn pack(piles: &[Vec<u8>]) -> Box<[u8]> {
    let mut v = Vec::with_capacity(piles[..STORES].iter().map(|p| p.len() + 1).sum::<usize>() + PILES - FOUNDATION);
    for p in &piles[..STORES] {
        v.extend_from_slice(p);
        v.push(0);
    }
    v.extend(piles[FOUNDATION..].iter().map(|p| p.first().copied().unwrap_or(0)));
    v.into_boxed_slice()
}

fn unpack(state: &[u8], piles: &mut [Vec<u8>]) {
    let mut i = 0;
    for p in &mut piles[..STORES] {
        p.clear();
        while state[i] != 0 {
            p.push(state[i]);
            i += 1;
        }
        i += 1;
    }
    for (p, &king) in piles[FOUNDATION..].iter_mut().zip(&state[i..]) {
        p.clear();
        if king != 0 {
            p.extend((1..=13).rev().map(|r| (king & !15) | r));
        }
    }
}

/// Clé de la position, aux symétries près : colonnes triées ; les
/// fondations, réduites aux couleurs déjà montées.
fn key(piles: &[Vec<u8>]) -> u64 {
    let mut h = FNV_START;
    let mut cols: Vec<&Vec<u8>> = piles[..STORES].iter().collect();
    cols.sort_unstable();
    for c in cols {
        fnv(&mut h, c);
        fnv(&mut h, &[0]);
    }
    let mut done: Vec<u8> = piles[FOUNDATION..].iter().filter_map(|p| p.first().map(|&c| suit(c))).collect();
    done.sort_unstable();
    fnv(&mut h, &done);
    h
}

/// Plus c'est petit, mieux c'est : les cassures dans les colonnes (rang qui
/// ne suit pas : 2 ; couleur qui change : 1), moins les colonnes vides.
fn score(piles: &[Vec<u8>]) -> i32 {
    let mut s = 0;
    for col in &piles[..STORES] {
        if col.is_empty() {
            s -= 2;
            continue;
        }
        for w in col.windows(2) {
            if rank(w[1]) + 1 != rank(w[0]) {
                s += 2;
            } else if suit(w[1]) != suit(w[0]) {
                s += 1;
            }
        }
    }
    let done = piles[FOUNDATION..].iter().filter(|p| !p.is_empty()).count() as i32;
    s - 8 * done
}

/// Les coups depuis la position (hors suites complètes, déjà montées).
fn moves(piles: &[Vec<u8>]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    let first_empty = (0..STORES).find(|&c| piles[c].is_empty());
    for from in 0..STORES {
        let pile = &piles[from];
        if pile.is_empty() {
            continue;
        }
        // Le début de la suite descendante du dessus.
        let mut start = pile.len() - 1;
        while start > 0 && rank(pile[start]) + 1 == rank(pile[start - 1]) {
            start -= 1;
        }
        for to in 0..STORES {
            if to == from {
                continue;
            }
            match piles[to].last() {
                None => {
                    if Some(to) != first_empty {
                        continue;
                    }
                    // Toute une colonne vers une colonne vide : inutile.
                    for index in start.max(1)..pile.len() {
                        if simon::can_drop(piles, from, index, to) {
                            out.push((from, index, to));
                        }
                    }
                }
                Some(&t) => {
                    if let Some(index) = (start..pile.len()).find(|&i| rank(pile[i]) + 1 == rank(t))
                        && simon::can_drop(piles, from, index, to)
                    {
                        out.push((from, index, to));
                    }
                }
            }
        }
    }
    out
}

struct Search<'a> {
    nodes: Vec<Node>,
    seen: HashSet<u64, BuildHasherDefault<IdentityHasher>>,
    heap: BinaryHeap<Entry>,
    counter: u32,
    stop: &'a AtomicBool,
}

impl Search<'_> {
    fn push_node(&mut self, parent: u32, from: usize, index: usize, to: usize) -> u32 {
        self.nodes.push(Node { parent, from: from as u8, index: index as u8, to: to as u8 });
        (self.nodes.len() - 1) as u32
    }

    fn apply(piles: &mut [Vec<u8>], from: usize, index: usize, to: usize) {
        let moving = piles[from].split_off(index);
        piles[to].extend(moving);
    }

    /// Monte les suites complètes, chacune dans un nœud à part.
    fn automoves(&mut self, piles: &mut [Vec<u8>], mut node: u32) -> u32 {
        while let Some(Move::Cards { from, index, to }) = simon::autodrop(piles) {
            Self::apply(piles, from, index, to);
            node = self.push_node(node, from, index, to);
        }
        node
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

    fn run(&mut self, mut piles: Vec<Vec<u8>>) -> Outcome {
        let won = |p: &[Vec<u8>]| p[FOUNDATION..].iter().all(|f| !f.is_empty());
        let node = self.automoves(&mut piles, ROOT);
        if won(&piles) {
            return Outcome::Winnable(self.path(node));
        }
        self.seen.insert(key(&piles));
        self.heap.push(Entry { score: Reverse((score(&piles), 0)), node, depth: 0, state: pack(&piles) });
        let mut base = piles.clone();
        let mut work = piles;
        let mut expanded = 0usize;
        while let Some(entry) = self.heap.pop() {
            expanded += 1;
            if expanded.is_multiple_of(1024) && self.stop.load(Ordering::Relaxed) {
                return Outcome::Aborted;
            }
            if self.seen.len() > MAX_POSITIONS {
                return Outcome::Unknown;
            }
            unpack(&entry.state, &mut base);
            for (from, index, to) in moves(&base) {
                work.clone_from(&base);
                Self::apply(&mut work, from, index, to);
                let mark = self.nodes.len();
                let mut node = self.push_node(entry.node, from, index, to);
                node = self.automoves(&mut work, node);
                if !self.seen.insert(key(&work)) {
                    // Position déjà vue : ses nœuds ne serviront jamais.
                    self.nodes.truncate(mark);
                    continue;
                }
                if won(&work) {
                    return Outcome::Winnable(self.path(node));
                }
                self.counter += 1;
                let depth = entry.depth + 1;
                self.heap.push(Entry { score: Reverse((score(&work) + depth, self.counter)), node, depth, state: pack(&work) });
            }
        }
        Outcome::NotWinnable
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    let piles: Vec<Vec<u8>> = game.piles.iter().map(|p| p.iter().map(|c| c.code()).collect()).collect();
    debug_assert_eq!(piles.len(), PILES);
    let mut search = Search { nodes: Vec::new(), seen: HashSet::default(), heap: BinaryHeap::new(), counter: 0, stop };
    search.run(piles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in 1..=20 {
            let g = Game::new(deal);
            if let Outcome::Winnable(moves) = solve(&g, &stop) {
                let mut end = g.clone();
                for (k, &m) in moves.iter().enumerate() {
                    assert!(end.play(m), "donne {deal}, coup {k} illégal : {m:?}");
                }
                assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                solved += 1;
            }
        }
        assert!(solved >= 15, "{solved} donnes résolues sur 20");
    }

    /// Vérification : recherche exhaustive naïve (tous les coups légaux,
    /// positions exactes). `cargo test --release -- --ignored`.
    fn brute_force(g: &Game, limit: usize) -> Option<bool> {
        let start: Vec<Vec<u8>> = g.piles.iter().map(|p| p.iter().map(|c| c.code()).collect()).collect();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![start];
        while let Some(piles) = stack.pop() {
            if piles[FOUNDATION..].iter().all(|f| !f.is_empty()) {
                return Some(true);
            }
            if !seen.insert(piles.clone()) {
                continue;
            }
            if seen.len() > limit {
                return None;
            }
            for from in 0..STORES {
                for index in 0..piles[from].len() {
                    for to in 0..PILES {
                        if simon::is_legal(&piles, from, index, to) {
                            let mut next = piles.clone();
                            let moving = next[from].split_off(index);
                            next[to].extend(moving);
                            if !seen.contains(&next) {
                                stack.push(next);
                            }
                        }
                    }
                }
            }
        }
        Some(false)
    }

    #[test]
    fn pack_round_trips() {
        let g = Game::new(1);
        let piles: Vec<Vec<u8>> = g.piles.iter().map(|p| p.iter().map(|c| c.code()).collect()).collect();
        let mut out = vec![Vec::new(); PILES];
        unpack(&pack(&piles), &mut out);
        assert_eq!(out, piles);
        // Une fondation montée (Roi → As de trèfle, code de couleur 2).
        let mut piles = piles;
        piles[FOUNDATION + 1] = (1..=13).rev().map(|r| r | (2 << 4)).collect();
        unpack(&pack(&piles), &mut out);
        assert_eq!(out, piles);
    }

    #[test]
    #[ignore]
    fn lost_deals_agree_with_brute_force() {
        let stop = AtomicBool::new(false);
        for deal in [6u32, 18, 46, 53] {
            let g = Game::new(deal);
            assert_eq!(solve(&g, &stop), Outcome::NotWinnable);
            let b = brute_force(&g, 2_000_000);
            eprintln!("donne {deal} : exhaustif {b:?}");
            assert_ne!(b, Some(true), "donne {deal} : gagnable");
        }
    }
}
