//! Outil de résolution de Quarante & Huit : portage de `FortyeightSolver`
//! de KPatience (© 2006-2009 Stephan Kulow, GPL-2.0-or-later) sur le moteur
//! `patsolve`, avec ses priorités et ses élagages.
//!
//! Corrigé par rapport à l'original :
//! - les sorties : `checkMoveOut` choisit mal la fondation d'un As (le
//!   second As d'une couleur ne sort jamais, ou s'empile sur une fondation
//!   commencée) ; ici, une carte sort sur la première fondation du jeu qui
//!   la prend ;
//! - le rebut retourné en talon (`make_move` le retournait et piochait
//!   aussitôt, en un coup) : ici deux coups, comme dans le jeu.
//!
//! Piles de travail : les 8 colonnes, le rebut (8) et le talon (9).

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::{Card, Suit};
use crate::fortyeight::{FOUNDATION, Game, STORES, TALON, WASTE};
use crate::patsolve::{self, Piles};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

const PILE: usize = STORES;
const DECK: usize = STORES + 1;
const ACE: u8 = 1;
const KING: u8 = 13;

// Codage de patsolve : (couleur << 4) + rang.
fn rank(c: u8) -> u8 {
    c & 0xF
}
fn suit(c: u8) -> u8 {
    c >> 4
}

fn code(c: &Card) -> u8 {
    let s = match c.suit {
        Suit::Diamonds => 0,
        Suit::Clubs => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    };
    s << 4 | c.rank
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Vers la fondation `f` du jeu (0 à 7).
    Out {
        from: usize,
        f: usize,
    },
    /// `card_index` + 1 cartes de `from` vers la colonne `to`.
    Store {
        from: usize,
        to: usize,
        card_index: usize,
    },
    Draw,
    Redeal,
}

#[derive(Clone, Copy, Debug)]
struct Mv {
    kind: Kind,
    pri: i32,
}

/// `FortyeightSolverState`.
struct Info {
    empty: [bool; STORES],
    linedup: [bool; STORES],
    highestbreak: [usize; STORES],
    firstempty: Option<usize>,
    freestores: usize,
    fromscore: [i32; STORES],
}

/// Une position, sans allocation.
#[derive(Clone, Copy)]
struct State {
    w: Piles<{ STORES + 2 }, 104>,
    /// Le dessus de chaque fondation du jeu (0 : vide).
    f: [u8; 8],
    lastdeal: bool,
}

impl State {
    fn from_game(g: &Game) -> State {
        let piles = (0..STORES).chain([WASTE, TALON]).map(|p| g.piles[p].iter().map(code));
        let f = std::array::from_fn(|i| g.piles[FOUNDATION + i].last().map_or(0, code));
        State { w: Piles::new(piles), f, lastdeal: g.lastdeal }
    }

    fn pile(&self, i: usize) -> &[u8] {
        self.w.pile(i)
    }

    fn top(&self, i: usize) -> Option<u8> {
        self.pile(i).last().copied()
    }

    /// Les fondations d'une même couleur sont interchangeables : leurs
    /// hauteurs, triées, par couleur.
    fn heights(&self) -> [u8; 8] {
        let mut h = [0u8; 8];
        for s in 0..4u8 {
            let mut r: Vec<u8> = self.f.iter().filter(|&&c| c != 0 && suit(c) == s).map(|&c| rank(c)).collect();
            r.sort_unstable_by(|a, b| b.cmp(a));
            for (k, x) in r.into_iter().take(2).enumerate() {
                h[s as usize * 2 + k] = x;
            }
        }
        h
    }

    fn outs(&self) -> i32 {
        self.f.iter().map(|&c| rank(c) as i32).sum()
    }

    fn cluster(&self) -> u32 {
        self.heights().iter().fold(0, |k, &x| k << 4 | x as u32)
    }

    fn is_won(&self) -> bool {
        self.f.iter().all(|&c| rank(c) == KING)
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        self.w.hash(&mut h);
        patsolve::fnv(&mut h, &self.heights());
        patsolve::fnv(&mut h, &[self.lastdeal as u8]);
        h
    }

    /// La première fondation du jeu qui prend `card`.
    fn foundation_for(&self, card: u8) -> Option<usize> {
        self.f.iter().position(|&t| if rank(card) == ACE { t == 0 } else { t != 0 && t + 1 == card })
    }

    /// `FortyeightSolver::checkState`.
    fn info(&self) -> Info {
        let mut d = Info {
            empty: [false; STORES],
            linedup: [true; STORES],
            highestbreak: [0; STORES],
            firstempty: None,
            freestores: 0,
            fromscore: [0; STORES],
        };
        for w in 0..STORES {
            let p = self.pile(w);
            let len = p.len();
            d.empty[w] = len == 0;
            if len == 0 {
                d.freestores += 1;
                d.firstempty.get_or_insert(w);
            }
            for i in 1..len {
                let (c1, c2) = (p[len - i - 1], p[len - i]);
                if suit(c1) != suit(c2) || rank(c1) != rank(c2) + 1 {
                    d.highestbreak[w] = i - 1;
                    d.linedup[w] = false;
                    break;
                }
            }
            for i in d.highestbreak[w] + 1..len {
                let r = rank(p[len - i - 1]) as i32;
                if r < 5 || r == ACE as i32 {
                    d.fromscore[w] += (13 - r) * 20;
                }
            }
            if len > 0 {
                d.fromscore[w] /= len as i32;
            }
        }
        d
    }

    /// `FortyeightSolver::get_possible_moves`.
    fn possible_moves(&self) -> Vec<Mv> {
        let mut out: Vec<Mv> = Vec::new();
        let d = self.info();

        // Sorties : celle de la première colonne qui en a une, et celle du
        // rebut. Seule à viser sa fondation, une sortie est jouée d'office ;
        // si les deux visent la même (deux cartes identiques), elles passent
        // en tête des autres coups.
        let outs: Vec<(usize, usize)> = (0..STORES)
            .find_map(|w| Some((w, self.foundation_for(self.top(w)?)?)))
            .into_iter()
            .chain(self.top(PILE).and_then(|c| self.foundation_for(c)).map(|f| (PILE, f)))
            .collect();
        match outs.as_slice() {
            [] => {}
            [(a, fa), (b, fb)] if fa == fb => {
                for &from in [a, b] {
                    let pri = (110 + self.pile(from).len() as i32).clamp(121, 126);
                    out.push(Mv { kind: Kind::Out { from, f: *fa }, pri });
                }
            }
            [(from, f), ..] => return vec![Mv { kind: Kind::Out { from: *from, f: *f }, pri: 127 }],
        }

        for w in 0..STORES {
            let pw = self.pile(w);
            for j in 0..STORES {
                if j == w {
                    continue;
                }
                // `checkMove` : la carte du dessus de w sur j.
                let Some(card) = self.top(w) else { continue };
                let pj = self.pile(j);
                if pj.last().is_some_and(|&t| suit(t) != suit(card) || rank(t) != rank(card) + 1) {
                    continue;
                }
                let pri = if pj.is_empty() {
                    if d.linedup[w] || Some(j) != d.firstempty || d.highestbreak[w] >= 1 {
                        continue;
                    }
                    (11 + d.fromscore[w]).min(115)
                } else {
                    if d.linedup[w] && pw.len() > 1 {
                        continue;
                    }
                    if d.linedup[j] && pj.len() > 1 {
                        110 + pj.len() as i32
                    } else if d.fromscore[w] < d.fromscore[j] {
                        2
                    } else {
                        (20 + d.highestbreak[w] as i32 + rank(card) as i32 + d.fromscore[w]).min(115)
                    }
                };
                out.push(Mv { kind: Kind::Store { from: w, to: j, card_index: 0 }, pri });
            }

            // Le rebut sur w.
            if let Some(card) = self.top(PILE) {
                let fits = pw.last().is_none_or(|&t| suit(t) == suit(card) && rank(t) == rank(card) + 1);
                if fits && (!d.empty[w] || Some(w) == d.firstempty) {
                    let pri = if d.empty[w] {
                        15
                    } else if d.linedup[w] {
                        100
                    } else {
                        (50 - d.fromscore[w]).max(0)
                    };
                    out.push(Mv { kind: Kind::Store { from: PILE, to: w, card_index: 0 }, pri });
                }
            }

            // Suites de plusieurs cartes, s'il y a une colonne vide.
            let len = pw.len();
            if len > 1 && d.freestores > 0 {
                let top = pw[len - 1];
                let next = pw[len - 2];
                if suit(next) != suit(top) || rank(next) != rank(top) + 1 {
                    continue;
                }
                for to in 0..STORES {
                    if to == w {
                        continue;
                    }
                    let pt = self.pile(to);
                    if pt.last().is_some_and(|&t| suit(t) != suit(top)) {
                        continue;
                    }
                    if d.empty[to] && Some(to) != d.firstempty {
                        continue;
                    }
                    let mut moves = if pt.is_empty() { 1 << (d.freestores - 1) } else { 1 << d.freestores };
                    moves = moves.min(len);
                    let mut switched = false;
                    for i in 2..moves {
                        let (below, above) = (pw[len - i - 1], pw[len - i]);
                        if suit(below) != suit(top) || rank(below) != rank(above) + 1 {
                            switched = true;
                            moves = i;
                            break;
                        }
                    }
                    if pt.is_empty() {
                        if moves < 2 || moves == len || !switched {
                            continue;
                        }
                        out.push(Mv { kind: Kind::Store { from: w, to, card_index: moves - 1 }, pri: 60 });
                        continue;
                    }
                    if d.linedup[w] && moves < len {
                        continue;
                    }
                    let t = pt[pt.len() - 1];
                    for i in 2..=moves.min(len) {
                        if rank(t) == rank(pw[len - i]) + 1 {
                            out.push(Mv { kind: Kind::Store { from: w, to, card_index: i - 1 }, pri: 80 });
                        }
                    }
                }
            }
        }

        if !self.pile(DECK).is_empty() {
            out.push(Mv { kind: Kind::Draw, pri: 19 });
        } else if !self.lastdeal && !self.pile(PILE).is_empty() {
            out.push(Mv { kind: Kind::Redeal, pri: 19 });
        }
        out
    }

    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        match m.kind {
            Kind::Out { from, f } => {
                s.f[f] = self.top(from).expect("carte");
                s.w.remove(from, 1);
            }
            Kind::Store { from, to, card_index } => s.w.transfer(from, to, card_index + 1),
            Kind::Draw => s.w.transfer(DECK, PILE, 1),
            Kind::Redeal => {
                // Le talon est vide : le rebut retourné devient le talon.
                let (start, end) = (s.w.start(PILE), s.w.ends[PILE] as usize);
                s.w.cards[start..end].reverse();
                s.w.ends[PILE] = start as u8;
                s.lastdeal = true;
            }
        }
        s
    }

    fn game_pile(p: usize) -> usize {
        if p == PILE { WASTE } else { p }
    }

    fn game_move(&self, m: &Mv) -> Move {
        match m.kind {
            Kind::Out { from, f } => Move::Cards { from: State::game_pile(from), index: self.pile(from).len() - 1, to: FOUNDATION + f },
            Kind::Store { from, to, card_index } => {
                Move::Cards { from: State::game_pile(from), index: self.pile(from).len() - card_index - 1, to }
            }
            Kind::Draw | Kind::Redeal => Move::Deal,
        }
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
    s.possible_moves().iter().map(|m| s.game_move(m)).collect()
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
        assert!(solved >= 5, "{solved} donnes résolues sur 20");
    }

    #[test]
    fn useful_moves_are_legal() {
        for deal in 1..=20 {
            let mut g = Game::new(deal);
            for _ in 0..30 {
                for m in useful_moves(&g) {
                    assert!(g.is_legal(m), "donne {deal} : {m:?}");
                }
                let Some(&m) = useful_moves(&g).first() else { break };
                g.play(m);
            }
        }
    }

    /// Les deux As d'une couleur sortent (l'original n'en sortait qu'un).
    #[test]
    fn both_aces_go_out() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        let card = |rank, suit| Card { id: 0, rank, suit, face_up: true };
        for (i, f) in (FOUNDATION..FOUNDATION + 8).enumerate() {
            g.piles[f] = (1..=13).map(|r| card(r, Suit::ALL[i / 2])).collect();
        }
        g.piles[FOUNDATION] = vec![card(1, Suit::Clubs)];
        g.piles[FOUNDATION + 1].clear();
        g.piles[0] = (2..=13).rev().map(|r| card(r, Suit::Clubs)).collect();
        g.piles[1] = (2..=13).rev().map(|r| card(r, Suit::Clubs)).collect();
        g.piles[2] = vec![card(1, Suit::Clubs)];
        g.lastdeal = true;
        let stop = AtomicBool::new(false);
        let Outcome::Winnable(moves) = solve(&g, &stop) else { panic!("gagnable") };
        let mut end = g.clone();
        assert!(moves.iter().all(|&m| end.play(m)) && end.is_won());
    }
}
