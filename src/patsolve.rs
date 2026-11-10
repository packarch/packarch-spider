//! Le moteur de recherche `patsolve` de KPatience (© 1998-2002 Tom Holroyd,
//! © 2006-2009 Stephan Kulow, GPL-2.0-or-later), commun aux outils de
//! résolution qui en dérivent (Klondike, Horloge du grand-père).
//!
//! Recherche en largeur avec priorités : les positions sont rangées dans
//! des files selon la priorité du coup et le nombre de cartes sorties, et
//! parcourues par un tourniquet qui favorise les files prioritaires ; une
//! position où une carte vient de sortir (changement de « grappe ») est
//! explorée tout de suite. Les positions déjà vues sont écartées.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::board::{Move, Outcome};

const NQUEUES: usize = 127;

/// Une position, vue par un jeu (ce que `KlondikeSolver`, `ClockSolver`…
/// fournissent au moteur).
pub trait Position: Clone {
    type Mv;
    /// Les coups à essayer, dans l'ordre de `Solver::get_moves`, avec leur
    /// priorité.
    fn moves(&self) -> Vec<(Self::Mv, i32)>;
    fn apply(&self, m: &Self::Mv) -> Self;
    /// Le coup du jeu correspondant (calculé avant de le jouer).
    fn game_move(&self, m: &Self::Mv) -> Move;
    /// Cartes sorties.
    fn outs(&self) -> i32;
    /// Contenu des fondations : un changement de grappe est exploré tout de suite.
    fn cluster(&self) -> u32;
    fn key(&self) -> u64;
    fn is_won(&self) -> bool;
}

/// FNV-1a 64 bits, pour les clés des positions.
pub fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

pub const FNV_START: u64 = 0xcbf2_9ce4_8422_2325;

/// Codage de patsolve : carte face cachée.
pub const DOWN: u8 = 1 << 7;

/// Les `N` piles de travail d'une position, sans allocation : leurs cartes
/// (au plus `C`) bout à bout dans `cards` (0 au-delà), la pile `i` finissant
/// à `ends[i]`.
#[derive(Clone, Copy)]
pub struct Piles<const N: usize, const C: usize = 52> {
    pub cards: [u8; C],
    pub ends: [u8; N],
}

impl<const N: usize, const C: usize> Piles<N, C> {
    pub fn new<I: IntoIterator<Item = u8>>(piles: impl IntoIterator<Item = I>) -> Piles<N, C> {
        let mut p = Piles { cards: [0; C], ends: [0; N] };
        let mut n = 0;
        for (end, pile) in p.ends.iter_mut().zip(piles) {
            for c in pile {
                p.cards[n] = c;
                n += 1;
            }
            *end = n as u8;
        }
        p
    }

    pub fn start(&self, i: usize) -> usize {
        if i == 0 { 0 } else { self.ends[i - 1] as usize }
    }

    pub fn pile(&self, i: usize) -> &[u8] {
        &self.cards[self.start(i)..self.ends[i] as usize]
    }

    /// Les `k` cartes du dessus de `from`, posées sur `to` dans le même ordre.
    pub fn transfer(&mut self, from: usize, to: usize, k: usize) {
        let (a, b) = (from.min(to), from.max(to));
        let region = &mut self.cards[self.ends[a] as usize - if from < to { k } else { 0 }..self.ends[b] as usize];
        if from < to {
            // [k cartes][piles a+1..=b] → [piles a+1..=b][k cartes]
            region.rotate_left(k);
            for e in &mut self.ends[a..b] {
                *e -= k as u8;
            }
        } else {
            // [piles a+1..b][k cartes] → [k cartes][piles a+1..b]
            region.rotate_right(k);
            for e in &mut self.ends[a..b] {
                *e += k as u8;
            }
        }
    }

    /// Retire les `k` cartes du dessus de `from` (sorties).
    pub fn remove(&mut self, from: usize, k: usize) {
        let (end, total) = (self.ends[from] as usize, self.ends[N - 1] as usize);
        self.cards.copy_within(end..total, end - k);
        self.cards[total - k..total].fill(0);
        for e in &mut self.ends[from..] {
            *e -= k as u8;
        }
    }

    /// Retourne face visible la carte du dessus de `i`.
    pub fn turn_top(&mut self, i: usize) {
        if self.ends[i] as usize > self.start(i) {
            self.cards[self.ends[i] as usize - 1] &= !DOWN;
        }
    }

    /// Ajoute les piles, chacune suivie d'un 0, au hachage `h`.
    pub fn hash(&self, h: &mut u64) {
        for i in 0..N {
            fnv(h, self.pile(i));
            fnv(h, &[0]);
        }
    }
}

#[derive(Clone, Copy)]
struct Node {
    parent: u32,
    mv: Move,
}

const ROOT: u32 = u32::MAX;

/// Les clés sont déjà des hachages : inutile de les rehacher.
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

enum Status {
    Searching,
    Won(u32),
    Aborted,
    Limit,
}

struct Search<'a, P> {
    nodes: Vec<Node>,
    seen: HashSet<u64, BuildHasherDefault<IdentityHasher>>,
    queues: Vec<Vec<(u32, P)>>,
    maxq: i32,
    qpos: i32,
    minpos: i32,
    status: Status,
    stop: &'a AtomicBool,
    calls: usize,
    max_positions: usize,
}

impl<P: Position> Search<'_, P> {
    /// `Solver::new_position` : la position est-elle nouvelle ?
    fn new_position(&mut self, s: &P, parent: u32, mv: Move) -> Option<u32> {
        if !self.seen.insert(s.key()) {
            return None;
        }
        if self.seen.len() > self.max_positions {
            self.status = Status::Limit;
        }
        self.nodes.push(Node { parent, mv });
        Some((self.nodes.len() - 1) as u32)
    }

    /// `Solver::queue_position` : la priorité du coup, plus une priorité
    /// selon le nombre de cartes sorties.
    fn queue(&mut self, node: u32, s: P, pri: i32) {
        let nout = s.outs() as f64;
        let x = (0.0032 * nout + 0.32) * nout - 3.0;
        let pri = (pri + (x + 0.5).floor() as i32).clamp(0, NQUEUES as i32 - 1);
        self.maxq = self.maxq.max(pri);
        self.queues[pri as usize].push((node, s));
    }

    /// `Solver::dequeue_position` : un tourniquet qui passe plus souvent sur
    /// les files prioritaires sans délaisser les autres.
    fn dequeue(&mut self) -> Option<(u32, P)> {
        let mut last = false;
        loop {
            self.qpos -= 1;
            if self.qpos < self.minpos {
                if last {
                    return None;
                }
                self.qpos = self.maxq;
                self.minpos -= 1;
                if self.minpos < 0 {
                    self.minpos = self.maxq;
                }
                if self.minpos == 0 {
                    last = true;
                }
            }
            if self.qpos >= 0 && !self.queues[self.qpos as usize].is_empty() {
                break;
            }
        }
        let entry = self.queues[self.qpos as usize].pop();
        while self.queues[self.qpos as usize].is_empty() && self.qpos == self.maxq && self.maxq > 0 {
            self.maxq -= 1;
            self.qpos -= 1;
            if self.qpos < self.minpos {
                self.minpos = self.qpos;
            }
        }
        entry
    }

    /// `Solver::solve`.
    fn solve(&mut self, node: u32, s: &P) {
        if !matches!(self.status, Status::Searching) {
            return;
        }
        self.calls += 1;
        if self.calls.is_multiple_of(1024) && self.stop.load(Ordering::Relaxed) {
            self.status = Status::Aborted;
            return;
        }
        let moves = s.moves();
        if moves.is_empty() {
            if s.is_won() {
                self.status = Status::Won(node);
            }
            return;
        }
        for (m, pri) in &moves {
            let next = s.apply(m);
            let gm = s.game_move(m);
            let Some(child) = self.new_position(&next, node, gm) else { continue };
            if next.cluster() != s.cluster() {
                // Une carte est sortie : on continue tout de suite.
                self.solve(child, &next);
            } else {
                self.queue(child, next, *pri);
            }
            if !matches!(self.status, Status::Searching) {
                return;
            }
        }
    }

    fn path(&self, mut node: u32) -> Vec<Move> {
        let mut moves = Vec::new();
        while node != ROOT {
            let n = self.nodes[node as usize];
            moves.push(n.mv);
            node = n.parent;
        }
        moves.reverse();
        moves
    }
}

/// `Solver::patsolve` : cherche une suite de coups gagnante depuis `start`.
/// Au-delà de `max_positions` positions, la recherche abandonne
/// (`Outcome::Unknown`).
pub fn solve<P: Position>(start: P, stop: &AtomicBool, max_positions: usize) -> Outcome {
    if start.is_won() {
        return Outcome::Winnable(Vec::new());
    }
    let mut search = Search {
        nodes: Vec::new(),
        seen: HashSet::default(),
        queues: (0..NQUEUES).map(|_| Vec::new()).collect(),
        maxq: 0,
        qpos: 0,
        minpos: 0,
        status: Status::Searching,
        stop,
        calls: 0,
        max_positions,
    };
    search.seen.insert(start.key());
    search.queue(ROOT, start, 0);
    while let Some((node, s)) = search.dequeue() {
        search.solve(node, &s);
        if !matches!(search.status, Status::Searching) {
            break;
        }
    }
    match search.status {
        Status::Won(node) => Outcome::Winnable(search.path(node)),
        Status::Searching => Outcome::NotWinnable,
        Status::Aborted => Outcome::Aborted,
        Status::Limit => Outcome::Unknown,
    }
}
