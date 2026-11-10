//! Outil de résolution du Yukon : portage de `YukonSolver` de KPatience
//! (© 2006-2009 Stephan Kulow, GPL-2.0-or-later) sur le moteur `patsolve`.
//!
//! Comme l'original, il connaît les cartes face cachée. Piles de travail :
//! les 7 colonnes ; les fondations sont réduites à la hauteur de chaque
//! couleur.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::Suit;
use crate::patsolve::{self, DOWN, Piles};
use crate::yukon::{FOUNDATION, Game, PILES, STORES};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang, couleurs
// dans l'ordre carreau, trèfle, cœur, pique (le bit 4 vaut « noir »).
const ACE: u8 = 1;
const KING: u8 = 13;

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

/// Un coup de patsolve (`MOVE`).
#[derive(Clone, Copy, Debug)]
struct Mv {
    /// Nombre de cartes au-dessus de celle qui part (0 : celle du dessus).
    card_index: usize,
    from: usize,
    to: usize,
    out: bool,
    pri: i32,
}

/// Une position, sans allocation.
#[derive(Clone, Copy)]
struct State {
    w: Piles<STORES>,
    /// Hauteur des fondations, par couleur (codage patsolve).
    o: [u8; 4],
    /// Pile de fondation du jeu attribuée à chaque couleur (pour traduire
    /// les coups) ; 0 : aucune encore.
    slot: [u8; 4],
}

impl State {
    fn from_game(g: &Game) -> State {
        let code = |c: &crate::cards::Card| (ps_suit(c.suit) << 4) | c.rank | if c.face_up { 0 } else { DOWN };
        let w = Piles::new((0..STORES).map(|i| g.piles[i].iter().map(code)));
        let mut o = [0; 4];
        let mut slot = [0; 4];
        for f in FOUNDATION..PILES {
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

    /// La pile de fondation du jeu pour la couleur `o` : celle qui lui est
    /// attribuée, sinon la première libre (`YukonSolver::translateMove`).
    fn foundation_pile(&self, o: usize) -> usize {
        if self.slot[o] != 0 {
            return self.slot[o] as usize;
        }
        (FOUNDATION..PILES).find(|&f| !self.slot.contains(&(f as u8))).expect("une fondation libre")
    }

    /// `YukonSolver::make_move`, sur une copie.
    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        if m.out {
            s.w.remove(m.from, 1);
            s.slot[m.to] = s.foundation_pile(m.to) as u8;
            s.o[m.to] += 1;
        } else {
            s.w.transfer(m.from, m.to, m.card_index + 1);
        }
        s.w.turn_top(m.from);
        s
    }

    /// Le coup du jeu correspondant.
    fn game_move(&self, m: &Mv) -> Move {
        let index = self.pile(m.from).len() - m.card_index - 1;
        let to = if m.out { self.foundation_pile(m.to) } else { m.to };
        Move::Cards { from: m.from, index, to }
    }

    /// `YukonSolver::good_automove` (règle de Raymond).
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

    /// `YukonSolver::get_possible_moves` : (coups, coup automatique ?,
    /// nombre de sorties en tête de liste).
    fn possible_moves(&self) -> (Vec<Mv>, bool, usize) {
        let w: [&[u8]; STORES] = std::array::from_fn(|i| self.pile(i));
        let mut out = Vec::new();
        for (i, pile) in w.iter().enumerate() {
            let Some(&card) = pile.last() else { continue };
            let o = suit(card);
            let empty = self.o[o] == 0;
            if (empty && rank(card) == ACE) || (!empty && rank(card) == self.o[o] + 1) {
                let m = Mv { card_index: 0, from: i, to: o, out: true, pri: 3 };
                if self.good_automove(o, rank(card)) {
                    return (vec![Mv { pri: 127, ..m }], true, 1);
                }
                out.push(m);
            }
        }
        let numout = out.len();

        for i in 0..STORES {
            let wlen = w[i].len();
            for l in 0..wlen {
                let card = w[i][wlen - 1 - l];
                if down(card) {
                    break;
                }
                for j in 0..STORES {
                    if i == j {
                        continue;
                    }
                    let fits = w[j].last().is_some_and(|&top| rank(card) + 1 == rank(top) && color(card) != color(top));
                    // Un Roi vers une colonne vide, sauf s'il est déjà au fond.
                    let king = rank(card) == KING && w[j].is_empty() && l != wlen - 1;
                    if fits || king {
                        let turns = wlen > l + 1 && down(w[i][wlen - l - 2]);
                        let pri = if turns || wlen == l + 1 { 30 } else { 1 };
                        out.push(Mv { card_index: l, from: i, to: j, out: false, pri });
                    }
                }
            }
        }
        (out, false, numout)
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    /// `Solver::get_moves` : les sorties qui ne sont pas automatiques passent
    /// après les autres coups.
    fn moves(&self) -> Vec<(Mv, i32)> {
        let (all, auto, numout) = self.possible_moves();
        let order: Vec<Mv> = if auto || numout == 0 { all } else { all[numout..].iter().chain(&all[..numout]).copied().collect() };
        order.into_iter().map(|m| (m, m.pri)).collect()
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
    let (moves, _, _) = s.possible_moves();
    moves.iter().map(|m| s.game_move(m)).collect()
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
        assert!(solved >= 10, "{solved} donnes résolues sur 20");
    }

    #[test]
    fn useful_moves_are_legal() {
        for deal in 1..=10 {
            let g = Game::new(deal);
            for m in useful_moves(&g) {
                assert!(g.is_legal(m), "donne {deal} : {m:?}");
            }
        }
    }
}
