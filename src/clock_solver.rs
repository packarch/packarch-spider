//! Outil de résolution de l'Horloge du grand-père : portage de
//! `ClockSolver` de KPatience (© 2006-2009 Stephan Kulow, GPL-2.0-or-later)
//! sur le moteur `patsolve`.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::clock::{FOUNDATION, Game, HOURS, STORES};
use crate::patsolve;

/// Au-delà, la recherche abandonne (KPatience s'arrête à 30 Mo de positions).
const MAX_POSITIONS: usize = 300_000;

fn code(c: &crate::cards::Card) -> u8 {
    c.rank | ((c.suit as u8) << 4)
}
fn rank(c: u8) -> u8 {
    c & 0xF
}
fn suit(c: u8) -> u8 {
    c >> 4
}

#[derive(Clone, Copy)]
struct Mv {
    from: usize,
    to: usize,
    out: bool,
}

#[derive(Clone)]
struct State {
    w: [Vec<u8>; STORES],
    /// La carte du dessus de chaque fondation.
    o: [u8; HOURS],
}

impl State {
    fn from_game(g: &Game) -> State {
        State {
            w: std::array::from_fn(|i| g.piles[i].iter().map(code).collect()),
            o: std::array::from_fn(|i| g.piles[FOUNDATION + i].last().map_or(0, code)),
        }
    }

    /// `ClockSolver::get_possible_moves` : (coups, nombre de sorties en tête).
    fn possible_moves(&self) -> (Vec<Mv>, usize) {
        let mut out = Vec::new();
        let mut first_empty = None;
        for w in 0..STORES {
            match self.w[w].last() {
                Some(&card) => {
                    for i in 0..HOURS {
                        let f = self.o[i];
                        if suit(card) != suit(f) {
                            continue;
                        }
                        let fits = if rank(card) == 1 { rank(f) == 13 } else { rank(f) + 1 == rank(card) };
                        if fits {
                            out.push(Mv { from: w, to: i, out: true });
                        }
                    }
                }
                None if first_empty.is_none() => first_empty = Some(w),
                None => {}
            }
        }
        let numout = out.len();
        for i in 0..STORES {
            let Some(&card) = self.w[i].last() else { continue };
            for j in 0..STORES {
                if i == j {
                    continue;
                }
                let allowed = match self.w[j].last() {
                    // Vers une pile vide : la première, et pas la dernière carte d'une pile.
                    None => self.w[i].len() > 1 && first_empty == Some(j),
                    Some(&below) => rank(card) + 1 == rank(below),
                };
                if allowed {
                    out.push(Mv { from: i, to: j, out: false });
                }
            }
        }
        (out, numout)
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    /// `Solver::get_moves` : pas de coup automatique ; les sorties passent
    /// après les autres coups.
    fn moves(&self) -> Vec<(Mv, i32)> {
        let (all, numout) = self.possible_moves();
        let pri = |m: &Mv| if m.out { 50 } else { 1 };
        all[numout..].iter().chain(&all[..numout]).map(|m| (*m, pri(m))).collect()
    }

    fn apply(&self, m: &Mv) -> State {
        let mut s = self.clone();
        let card = s.w[m.from].pop().expect("pile non vide");
        if m.out {
            let f = &mut s.o[m.to];
            *f = if rank(*f) == 13 { *f - 12 } else { *f + 1 };
        } else {
            s.w[m.to].push(card);
        }
        s
    }

    fn game_move(&self, m: &Mv) -> Move {
        let to = if m.out { FOUNDATION + m.to } else { m.to };
        Move::Cards { from: m.from, index: self.w[m.from].len() - 1, to }
    }

    fn outs(&self) -> i32 {
        52 - self.w.iter().map(Vec::len).sum::<usize>() as i32
    }

    /// Pas de grappes pour l'Horloge (`Solver::getClusterNumber` par défaut).
    fn cluster(&self) -> u32 {
        0
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        for p in &self.w {
            patsolve::fnv(&mut h, p);
            patsolve::fnv(&mut h, &[0]);
        }
        patsolve::fnv(&mut h, &self.o);
        h
    }

    fn is_won(&self) -> bool {
        self.w.iter().all(Vec::is_empty)
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    patsolve::solve(State::from_game(game), stop, MAX_POSITIONS)
}

/// Les coups utiles (ceux que l'outil de résolution essaierait), pour les
/// conseils quand il ne connaît pas de chemin gagnant.
pub fn useful_moves(game: &Game) -> Vec<Move> {
    let s = State::from_game(game);
    let (moves, _) = s.possible_moves();
    moves.iter().map(|m| patsolve::Position::game_move(&s, m)).collect()
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
}
