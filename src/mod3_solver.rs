//! Outil de résolution de Mod3 : portage de `Mod3Solver` de KPatience
//! (© 2006-2009 Stephan Kulow, GPL-2.0-or-later) sur le moteur `patsolve`.
//!
//! Comme l'original, il connaît l'ordre du talon. Piles de travail, dans
//! l'ordre du jeu : les 32 piles des 4 rangées, la pile des As, le talon.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::Suit;
use crate::mod3::{ACES, COLUMNS, Game, PILES, RESERVE, TALON};
use crate::patsolve::{self, DOWN, Piles};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang.
const ACE: u8 = 1;
const PS_SUIT: u8 = 0x30;

fn rank(c: u8) -> u8 {
    c & 0xF
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

/// Un coup de patsolve (`MOVE`) : une carte de `from` vers `to` (`refill` :
/// la pile de réserve vidée reprend la carte du talon), ou la distribution.
#[derive(Clone, Copy, Debug)]
struct Mv {
    from: usize,
    to: usize,
    deal: bool,
    refill: bool,
    pri: i32,
}

/// Une position, sans allocation (104 cartes, 34 piles).
#[derive(Clone, Copy)]
struct State {
    w: Piles<PILES, 104>,
}

impl State {
    fn from_game(g: &Game) -> State {
        let code = |c: &crate::cards::Card| (ps_suit(c.suit) << 4) | c.rank | if c.face_up { 0 } else { DOWN };
        State { w: Piles::new(g.piles.iter().map(|p| p.iter().map(code))) }
    }

    fn len(&self, p: usize) -> usize {
        self.w.pile(p).len()
    }

    fn top(&self, p: usize) -> u8 {
        *self.w.pile(p).last().expect("pile non vide")
    }

    /// `Mod3Solver::getOuts` : les cartes des fondations et des As.
    fn outs(&self) -> i32 {
        (self.len(ACES) + (0..RESERVE).map(|p| self.len(p)).sum::<usize>()) as i32
    }

    fn is_won(&self) -> bool {
        self.outs() == 104
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        self.w.hash(&mut h);
        h
    }

    /// `Mod3Solver::make_move`, sur une copie.
    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        if m.deal {
            for p in RESERVE..ACES {
                if s.len(TALON) == 0 {
                    break;
                }
                s.w.transfer(TALON, p, 1);
                s.w.turn_top(p);
            }
            return s;
        }
        s.w.transfer(m.from, m.to, 1);
        if m.refill {
            s.w.transfer(TALON, m.from, 1);
            s.w.turn_top(m.from);
        }
        s
    }

    fn game_move(&self, m: &Mv) -> Move {
        if m.deal {
            return Move::Deal;
        }
        Move::Cards { from: m.from, index: self.len(m.from) - 1, to: m.to }
    }

    /// `Mod3Solver::get_possible_moves` : un As va d'office sur sa pile ;
    /// sinon les coups vers les fondations, vers une pile vide de la
    /// réserve, et la distribution en dernier recours.
    fn possible_moves(&self) -> Vec<Mv> {
        let deck = self.len(TALON) > 0;
        let mut out = Vec::new();
        let mut firstfree: [Option<usize>; 4] = [None; 4];
        let mut first_empty_pile: Option<usize> = None;
        for i in 0..ACES {
            let li = self.len(i);
            if li == 0 {
                continue;
            }
            let card = self.top(i);
            let refill = i >= RESERVE && li == 1 && deck;
            if rank(card) == ACE {
                return vec![Mv { from: i, to: ACES, deal: false, refill, pri: 127 }];
            }
            let current_row = i / COLUMNS;
            for j in 0..ACES {
                if i == j {
                    continue;
                }
                let row = j / COLUMNS;
                let lj = self.len(j);
                if lj == 0 {
                    let first = *firstfree[row].get_or_insert(j);
                    if j != first {
                        continue;
                    }
                }
                if lj > 0 && rank(self.top(j)) as usize != row + 2 + (lj - 1) * 3 {
                    continue;
                }
                if row < 3 {
                    let min = if lj == 0 {
                        (card & PS_SUIT) + row as u8 + 2
                    } else {
                        let t = self.top(j);
                        if rank(t) > 10 {
                            continue;
                        }
                        (t & PS_SUIT) + row as u8 + 2 + lj as u8 * 3
                    };
                    if min != card {
                        continue;
                    }
                    // Échanger deux piles de la même rangée ne sert à rien.
                    if current_row == row && li == lj + 1 {
                        continue;
                    }
                    let pri = if lj > 0 {
                        (12 + 20 * lj as i32 + current_row as i32 * 2 + rank(self.top(j)) as i32 * 5).min(119)
                    } else {
                        119
                    };
                    out.push(Mv { from: i, to: j, deal: false, refill, pri });
                } else {
                    if lj > 0 || (li > 1 && current_row != 3) {
                        continue;
                    }
                    if current_row == 3 && li == 1 {
                        continue;
                    }
                    if rank(card) as usize == current_row + 2 && current_row != 3 {
                        continue;
                    }
                    if *first_empty_pile.get_or_insert(j) != j {
                        continue;
                    }
                    out.push(Mv { from: i, to: j, deal: false, refill: false, pri: 0 });
                }
            }
        }
        if deck {
            out.push(Mv { from: TALON, to: 0, deal: true, refill: false, pri: -77 });
        }
        out
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    fn moves(&self) -> Vec<(Mv, i32)> {
        self.possible_moves().into_iter().map(|m| (m, m.pri)).collect()
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
    /// Pas de grappes (`Solver::getClusterNumber` par défaut).
    fn cluster(&self) -> u32 {
        0
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
/// essaierait, sauf la distribution), pour les conseils quand il ne connaît
/// pas de chemin gagnant.
pub fn useful_moves(game: &Game) -> Vec<Move> {
    let s = State::from_game(game);
    s.possible_moves().iter().filter(|m| !m.deal).map(|m| s.game_move(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les coups du solveur sont ceux du jeu, distribution et remplissage
    /// de la réserve compris.
    #[test]
    fn moves_match_the_game() {
        for deal in 1..=5 {
            let mut g = Game::new(deal);
            for _ in 0..40 {
                let s = State::from_game(&g);
                let Some(m) = s.possible_moves().first().copied() else { break };
                let gm = s.game_move(&m);
                assert!(g.play(gm), "donne {deal} : {gm:?} illégal");
                let (a, b) = (s.apply(&m), State::from_game(&g));
                for p in 0..PILES {
                    assert_eq!(a.w.pile(p), b.w.pile(p), "donne {deal}, pile {p} après {gm:?}");
                }
            }
        }
    }

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in 1..=10 {
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
        assert!(solved >= 3, "{solved} donnes résolues sur 10");
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
