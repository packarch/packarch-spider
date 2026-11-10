//! Outil de résolution de Bizilizi : portage de `BiziliziSolver` de
//! KPatience (GPL-2.0-or-later) sur le moteur `patsolve`, avec ses priorités
//! et ses élagages.
//!
//! Comme l'original, il connaît les cartes face cachée. Piles de travail :
//! les 7 colonnes et la réserve (7), dont une carte quelconque part seule ;
//! les fondations sont réduites à la hauteur de chaque couleur. Une
//! différence : l'original n'envisage de sortir que la dernière carte de la
//! réserve, celui-ci n'importe laquelle (le jeu le permet).

use std::sync::atomic::AtomicBool;

use crate::bizilizi::{FOUNDATION, Game, POOL, STORES, preferred_foundation};
use crate::board::{Move, Outcome};
use crate::cards::Suit;
use crate::patsolve::{self, DOWN, Piles};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

const RES: usize = STORES;
const ACE: u8 = 1;
const KING: u8 = 13;

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang, couleurs
// dans l'ordre carreau, trèfle, cœur, pique (le bit 4 vaut « noir »).
fn rank(c: u8) -> u8 {
    c & 0xF
}
fn suit(c: u8) -> usize {
    ((c >> 4) & 3) as usize
}
fn color(c: u8) -> u8 {
    c & 0x10
}
fn down(c: u8) -> bool {
    c & DOWN != 0
}

/// `Solver::translateSuit` : trèfle et carreau échangés.
fn ps_suit(s: Suit) -> u8 {
    match s {
        Suit::Clubs => 1,
        Suit::Diamonds => 0,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    }
}

fn game_suit(o: usize) -> Suit {
    [Suit::Diamonds, Suit::Clubs, Suit::Hearts, Suit::Spades][o]
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// La carte `index` (depuis le dessus) de `from` vers la fondation de sa
    /// couleur.
    Out { from: usize, index: usize },
    /// `card_index` + 1 cartes de la colonne `from` (une seule, la
    /// `card_index`-ième depuis le dessus, pour la réserve) vers la colonne `to`.
    Store { from: usize, to: usize, card_index: usize },
}

#[derive(Clone, Copy, Debug)]
struct Mv {
    kind: Kind,
    pri: i32,
}

/// Une position, sans allocation.
#[derive(Clone, Copy)]
struct State {
    w: Piles<{ STORES + 1 }>,
    /// Hauteur des fondations, par couleur (codage patsolve).
    o: [u8; 4],
    /// Fondation du jeu de chaque couleur (0 : aucune encore).
    slot: [u8; 4],
}

impl State {
    fn from_game(g: &Game) -> State {
        let code = |c: &crate::cards::Card| (ps_suit(c.suit) << 4) | c.rank | if c.face_up { 0 } else { DOWN };
        let w = Piles::new((0..STORES).chain([POOL]).map(|i| g.piles[i].iter().map(code)));
        let mut o = [0; 4];
        let mut slot = [0; 4];
        for f in FOUNDATION..POOL {
            if let Some(t) = g.piles[f].last() {
                let s = ps_suit(t.suit) as usize;
                o[s] = t.rank;
                slot[s] = f as u8;
            }
        }
        State { w, o, slot }
    }

    fn pile(&self, i: usize) -> &[u8] {
        self.w.pile(i)
    }

    fn outs(&self) -> i32 {
        self.o.iter().map(|&x| x as i32).sum()
    }

    fn cluster(&self) -> u32 {
        self.o[0] as u32 | (self.o[1] as u32) << 4 | (self.o[2] as u32) << 8 | (self.o[3] as u32) << 12
    }

    fn is_won(&self) -> bool {
        self.o.iter().all(|&x| x == KING)
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        self.w.hash(&mut h);
        patsolve::fnv(&mut h, &self.o);
        h
    }

    /// La fondation du jeu pour la couleur `o` : la sienne, sinon la
    /// préférée si elle est libre, sinon la première libre (comme
    /// `Game::foundation_for`).
    fn foundation_pile(&self, o: usize) -> usize {
        if self.slot[o] != 0 {
            return self.slot[o] as usize;
        }
        let used = |f: usize| self.slot.contains(&(f as u8));
        let pref = preferred_foundation(game_suit(o));
        if !used(pref) {
            return pref;
        }
        (FOUNDATION..POOL).find(|&f| !used(f)).expect("une fondation libre")
    }

    fn fits_out(&self, card: u8) -> bool {
        let o = suit(card);
        (self.o[o] == 0 && rank(card) == ACE) || (self.o[o] != 0 && rank(card) == self.o[o] + 1)
    }

    /// `BiziliziSolver::good_automove` (règle de Raymond).
    fn good_automove(&self, o: usize, r: u8) -> bool {
        if r <= 2 {
            return true;
        }
        let mut i = 1 - (o & 1);
        while i < 4 {
            if self.o[i] + 1 < r {
                let mut j = 1 - (o & 1);
                while j < 4 {
                    if self.o[j] + 2 < r {
                        return false;
                    }
                    j += 2;
                }
                return self.o[(o + 2) & 3] + 3 >= r;
            }
            i += 2;
        }
        true
    }

    /// `BiziliziSolver::get_possible_moves` : (coups, nombre de sorties non
    /// automatiques en tête) ; une sortie automatique est seule.
    fn possible_moves(&self) -> (Vec<Mv>, usize) {
        let mut out = Vec::new();
        for w in 0..=RES {
            let p = self.pile(w);
            // Le dessus d'une colonne ; toute carte de la réserve.
            let indices = if w == RES { 0..p.len() } else { 0..p.len().min(1) };
            for index in indices {
                let card = p[p.len() - 1 - index];
                if !self.fits_out(card) {
                    continue;
                }
                let m = Mv { kind: Kind::Out { from: w, index }, pri: 3 };
                if self.good_automove(suit(card), rank(card)) {
                    return (vec![Mv { pri: 127, ..m }], 0);
                }
                out.push(m);
            }
        }
        let numout = out.len();

        let first_empty = (0..STORES).find(|&i| self.pile(i).is_empty());
        for i in 0..=RES {
            let p = self.pile(i);
            let len = p.len();
            for l in 0..len {
                let card = p[len - 1 - l];
                if down(card) {
                    break;
                }
                for j in 0..STORES {
                    if i == j {
                        continue;
                    }
                    let pj = self.pile(j);
                    let mut allowed = 0;
                    if let Some(&t) = pj.last()
                        && rank(card) + 1 == rank(t)
                        && color(card) != color(t)
                    {
                        allowed = if len == l + 1 {
                            2
                        } else if down(p[len - l - 2]) {
                            3
                        } else {
                            1
                        };
                    }
                    if rank(card) == KING && Some(j) == first_empty && (l != len - 1 || i == RES) {
                        allowed = 4;
                    }
                    if allowed != 0 && i == RES {
                        allowed = 5;
                    }
                    if allowed == 1 {
                        // Une partie seulement de la suite : utile seulement si
                        // la carte découverte sort.
                        if !self.fits_out(p[len - 2 - l]) {
                            allowed = 0;
                        }
                    }
                    if allowed == 0 {
                        continue;
                    }
                    let turns = len > l + 1 && down(p[len - l - 2]);
                    let pri = if i == RES {
                        40
                    } else if turns || len == l + 1 {
                        30
                    } else {
                        1
                    };
                    out.push(Mv { kind: Kind::Store { from: i, to: j, card_index: l }, pri });
                }
            }
        }
        (out, numout)
    }

    /// Retire la carte `index` (depuis le dessus) de la réserve, dernière pile.
    fn take_from_reserve(&mut self, index: usize) -> u8 {
        let total = self.w.ends[RES] as usize;
        let a = total - 1 - index;
        let card = self.w.cards[a];
        self.w.cards.copy_within(a + 1..total, a);
        self.w.cards[total - 1] = 0;
        self.w.ends[RES] -= 1;
        card
    }

    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        match m.kind {
            Kind::Out { from, index } => {
                let card = if from == RES {
                    s.take_from_reserve(index)
                } else {
                    let c = *s.pile(from).last().expect("carte");
                    s.w.remove(from, 1);
                    s.w.turn_top(from);
                    c
                };
                let o = suit(card);
                s.slot[o] = s.foundation_pile(o) as u8;
                s.o[o] += 1;
            }
            Kind::Store { from: RES, to, card_index } => {
                // La carte quitte la réserve (dernière pile) pour le dessus de
                // `to` : rotation de la zone entre les deux.
                let a = s.w.ends[RES] as usize - 1 - card_index;
                let end_to = s.w.ends[to] as usize;
                s.w.cards[end_to..=a].rotate_right(1);
                for e in &mut s.w.ends[to..RES] {
                    *e += 1;
                }
            }
            Kind::Store { from, to, card_index } => {
                s.w.transfer(from, to, card_index + 1);
                s.w.turn_top(from);
            }
        }
        s
    }

    fn game_move(&self, m: &Mv) -> Move {
        let game_pile = |p: usize| if p == RES { POOL } else { p };
        match m.kind {
            Kind::Out { from, index } => {
                let len = self.pile(from).len();
                let card = self.pile(from)[len - 1 - index];
                Move::Cards { from: game_pile(from), index: len - 1 - index, to: self.foundation_pile(suit(card)) }
            }
            Kind::Store { from, to, card_index } => {
                Move::Cards { from: game_pile(from), index: self.pile(from).len() - card_index - 1, to }
            }
        }
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    /// `Solver::get_moves` : les sorties qui ne sont pas automatiques passent
    /// après les autres coups.
    fn moves(&self) -> Vec<(Mv, i32)> {
        let (all, numout) = self.possible_moves();
        all[numout..].iter().chain(&all[..numout]).map(|&m| (m, m.pri)).collect()
    }
    fn apply(&self, m: &Mv) -> State {
        State::apply(self, m)
    }
    fn game_move(&self, m: &Mv) -> Move {
        State::game_move(self, m)
    }
    fn outs(&self) -> i32 {
        State::outs(self)
    }
    fn cluster(&self) -> u32 {
        State::cluster(self)
    }
    fn key(&self) -> u64 {
        State::key(self)
    }
    fn is_won(&self) -> bool {
        State::is_won(self)
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    patsolve::solve(State::from_game(game), stop, MAX_POSITIONS)
}

/// Les coups utiles de la position (ceux que l'outil de résolution
/// essaierait), pour les conseils quand il ne connaît pas de chemin gagnant.
pub fn useful_moves(game: &Game) -> Vec<Move> {
    let s = State::from_game(game);
    let (moves, _) = s.possible_moves();
    moves.iter().map(|m| s.game_move(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        for extra in [2, 3] {
            let mut solved = 0;
            for deal in 1..=20 {
                let g = Game::new(extra, deal);
                if let Outcome::Winnable(moves) = solve(&g, &stop) {
                    let mut end = g.clone();
                    for (k, &m) in moves.iter().enumerate() {
                        assert!(end.play(m), "{extra} en plus, donne {deal}, coup {k} illégal : {m:?}");
                    }
                    assert!(end.is_won(), "{extra} en plus, donne {deal} : la solution ne gagne pas");
                    solved += 1;
                }
            }
            assert!(solved >= 5, "{extra} en plus : {solved} donnes résolues sur 20");
        }
    }

    #[test]
    fn useful_moves_are_legal() {
        for deal in 1..=20 {
            let mut g = Game::new(2, deal);
            for _ in 0..30 {
                let moves = useful_moves(&g);
                for &m in &moves {
                    assert!(g.is_legal(m), "donne {deal} : {m:?}");
                }
                let Some(&m) = moves.first() else { break };
                g.play(m);
            }
        }
    }

    /// Le jeu et l'outil de résolution sont d'accord sur la règle de Raymond.
    #[test]
    fn automove_rules_agree() {
        for deal in 1..=30 {
            let mut g = Game::new(2, deal);
            for _ in 0..40 {
                let s = State::from_game(&g);
                for c in g.piles[..STORES].iter().filter_map(|p| p.last()).chain(&g.piles[POOL]) {
                    assert_eq!(s.good_automove(ps_suit(c.suit) as usize, c.rank), g.good_automove(c), "donne {deal}");
                }
                let Some(&m) = useful_moves(&g).first() else { break };
                g.play(m);
            }
        }
    }
}
