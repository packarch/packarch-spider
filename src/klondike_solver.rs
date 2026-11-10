//! Outil de résolution du Klondike : portage de `patsolve` (© 1998-2002 Tom
//! Holroyd, © 2006-2009 Stephan Kulow) et de `KlondikeSolver` de KPatience
//! (GPL-2.0-or-later).
//!
//! Le moteur est dans `patsolve.rs` ; ici, les coups du Klondike. Comme
//! l'original, il connaît les cartes face cachée et l'ordre du talon.
//!
//! Piles de travail : colonnes 0 à 6, pioche 7, talon 8 ; les fondations
//! sont réduites à la hauteur de chaque couleur.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::Suit;
use crate::klondike::{FOUNDATION, Game, PILES, STOCK, WASTE};
use crate::patsolve;

/// Au-delà, la recherche abandonne (KPatience s'arrête à 30 Mo de positions ;
/// ici ≈ 120 Mo et 0,6 s au pire sur les donnes 1 à 1000).
const MAX_POSITIONS: usize = 1_000_000;

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang, couleurs
// dans l'ordre carreau, trèfle, cœur, pique (le bit 4 vaut « noir »).
use crate::patsolve::{DOWN, Piles};
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

#[cfg(test)]
fn from_ps_suit(o: usize) -> Suit {
    [Suit::Diamonds, Suit::Clubs, Suit::Hearts, Suit::Spades][o]
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

/// Une position, sans allocation (70 octets en tout).
#[derive(Clone, Copy)]
struct State {
    /// Les 9 piles de travail.
    w: Piles<9>,
    /// Hauteur des fondations, par couleur (codage patsolve).
    o: [u8; 4],
    /// Pile de fondation du jeu attribuée à chaque couleur (pour traduire
    /// les coups) ; 0 : aucune encore.
    slot: [u8; 4],
    /// Cartes piochées à la fois.
    draw: u8,
}

impl State {
    fn from_game(g: &Game) -> State {
        let code = |c: &crate::cards::Card| (ps_suit(c.suit) << 4) | c.rank | if c.face_up { 0 } else { DOWN };
        let w = Piles::new((0..9).map(|i| g.piles[i].iter().map(code)));
        let mut o = [0; 4];
        let mut slot = [0; 4];
        for f in FOUNDATION..PILES {
            if let Some(t) = g.piles[f].last() {
                let s = ps_suit(t.suit) as usize;
                o[s] = t.rank;
                slot[s] = f as u8;
            }
        }
        State { w, o, slot, draw: g.draw }
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
    /// attribuée, sinon la première libre (`KlondikeSolver::translateMove`).
    fn foundation_pile(&self, o: usize) -> usize {
        if self.slot[o] != 0 {
            return self.slot[o] as usize;
        }
        (FOUNDATION..PILES).find(|&f| !self.slot.contains(&(f as u8))).expect("une fondation libre")
    }

    /// `KlondikeSolver::make_move`, sur une copie.
    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        let (from, to) = (m.from, m.to);
        if from == WASTE && to == STOCK {
            // Le talon est vide : la pioche retournée le devient.
            let (a, b) = (s.w.start(WASTE), s.w.ends[WASTE] as usize);
            s.w.cards[a..b].reverse();
            for c in &mut s.w.cards[a..b] {
                *c |= DOWN;
            }
            s.w.ends[WASTE] = a as u8;
            return s;
        }
        if from == STOCK && to == WASTE {
            for _ in 0..m.card_index {
                if s.w.ends[STOCK] > s.w.ends[WASTE] {
                    s.w.transfer(STOCK, WASTE, 1);
                    s.w.turn_top(WASTE);
                }
            }
            return s;
        }
        let k = m.card_index + 1;
        if m.out {
            s.w.remove(from, k);
            let pile = s.foundation_pile(to);
            s.slot[to] = pile as u8;
            s.o[to] += 1;
        } else {
            s.w.transfer(from, to, k);
        }
        s.w.turn_top(from);
        s
    }

    /// Le coup du jeu correspondant.
    fn game_move(&self, m: &Mv) -> Move {
        if m.from == STOCK || m.to == STOCK {
            return Move::Deal;
        }
        let index = self.pile(m.from).len() - m.card_index - 1;
        let to = if m.out { self.foundation_pile(m.to) } else { m.to };
        Move::Cards { from: m.from, index, to }
    }

    /// `KlondikeSolver::good_automove` (règle de Raymond).
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

    /// `KlondikeSolver::get_possible_moves` : (coups, coup automatique ?,
    /// nombre de sorties en tête de liste).
    fn possible_moves(&self, draw: usize) -> (Vec<Mv>, bool, usize) {
        let w: [&[u8]; 9] = std::array::from_fn(|i| self.pile(i));
        let mut out = Vec::new();
        for (i, pile) in w.iter().enumerate().take(8) {
            let Some(&card) = pile.last() else { continue };
            let o = suit(card);
            let empty = self.o[o] == 0;
            if (empty && rank(card) == ACE) || (!empty && rank(card) == self.o[o] + 1) {
                let m = Mv { card_index: 0, from: i, to: o, out: true, pri: 3 };
                // Sorties automatiques ; depuis la pioche en tirage 3, elles
                // décaleraient les paquets.
                if self.good_automove(o, rank(card)) && (i != WASTE || draw == 1 || pile.len() < 3) {
                    return (vec![Mv { pri: 127, ..m }], true, 1);
                }
                out.push(m);
            }
        }
        let numout = out.len();

        if !w[STOCK].is_empty() {
            out.push(Mv { card_index: draw.min(w[STOCK].len()), from: STOCK, to: WASTE, out: false, pri: 5 });
        }

        // Où poser un Roi : seulement dans la première colonne vide.
        let first_empty = (0..8).find(|&i| w[i].is_empty());
        for i in 0..8 {
            let wlen = w[i].len();
            let len = if i == WASTE && wlen > 0 { 1 } else { wlen };
            for l in 0..len {
                let card = w[i][wlen - 1 - l];
                if down(card) {
                    break;
                }
                for j in 0..7 {
                    if i == j {
                        continue;
                    }
                    let mut allowed = 0;
                    if let Some(&top) = w[j].last() {
                        if rank(card) + 1 == rank(top) && color(card) != color(top) {
                            allowed = if wlen == l + 1 {
                                2
                            } else if down(w[i][wlen - l - 2]) {
                                3
                            } else {
                                1
                            };
                        }
                    }
                    if rank(card) == KING && Some(j) == first_empty && (l != wlen - 1 || i == WASTE) {
                        allowed = 4;
                    }
                    if allowed != 0 && i == WASTE {
                        allowed = 5;
                    }
                    if allowed == 1 {
                        // Déplacer une partie d'une suite ne sert que si la
                        // carte découverte peut sortir.
                        let below = w[i][wlen - 2 - l];
                        let o = suit(below);
                        let empty = self.o[o] == 0;
                        if (empty && rank(below) != ACE) || (!empty && rank(below) != self.o[o] + 1) {
                            allowed = 0;
                        }
                    }
                    if allowed != 0 {
                        let turns = wlen > l + 1 && down(w[i][wlen - l - 2]);
                        let pri = if i == WASTE {
                            40
                        } else if turns || wlen == l + 1 {
                            30
                        } else {
                            1
                        };
                        out.push(Mv { card_index: l, from: i, to: j, out: false, pri });
                    }
                }
            }
        }

        if w[STOCK].is_empty() && w[WASTE].len() > 1 {
            out.push(Mv { card_index: 0, from: WASTE, to: STOCK, out: false, pri: 2 });
        }
        (out, false, numout)
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    /// `Solver::get_moves` : les sorties qui ne sont pas automatiques passent
    /// après les autres coups.
    fn moves(&self) -> Vec<(Mv, i32)> {
        let (all, auto, numout) = self.possible_moves(self.draw as usize);
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
    let (moves, _, _) = s.possible_moves(game.draw as usize);
    moves.iter().filter(|m| m.from != STOCK && m.to != STOCK).map(|m| s.game_move(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(draw: u8, deals: std::ops::RangeInclusive<u32>, min_solved: usize) {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in deals.clone() {
            let g = Game::new(draw, deal);
            if let Outcome::Winnable(moves) = solve(&g, &stop) {
                let mut end = g.clone();
                for (k, &m) in moves.iter().enumerate() {
                    assert!(end.play(m), "donne {deal}, coup {k} illégal : {m:?}");
                }
                assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                solved += 1;
            }
        }
        assert!(solved >= min_solved, "{solved} donnes résolues sur {}", deals.count());
    }

    #[test]
    fn draw_one_deals_are_solved_and_replayable() {
        check(1, 1..=20, 12);
    }

    #[test]
    fn draw_three_deals_are_solved_and_replayable() {
        check(3, 1..=20, 8);
    }

    fn piles(s: &State) -> Vec<Vec<u8>> {
        (0..9).map(|i| s.pile(i).to_vec()).collect()
    }

    #[test]
    fn pile_operations_match_vectors() {
        let s = State::from_game(&Game::new(1, 7));
        let mut v = piles(&s);
        // Vers une pile plus loin, puis vers une pile plus proche.
        for (from, to, k) in [(1, 5, 1), (6, 2, 3), (7, 0, 0), (8, 3, 2), (3, 8, 2)] {
            if k == 0 || k > v[from].len() {
                continue;
            }
            let mut t = s;
            t.w.transfer(from, to, k);
            let mut w = v.clone();
            let n = w[from].len();
            let moving = w[from].split_off(n - k);
            w[to].extend(moving);
            assert_eq!(piles(&t), w, "{from} → {to}, {k} cartes");
        }
        let mut t = s;
        t.w.remove(4, 2);
        let n = v[4].len();
        v[4].truncate(n - 2);
        assert_eq!(piles(&t), v);
        assert_eq!(t.w.cards[t.w.ends[8] as usize..], [0, 0][..]);
    }

    #[test]
    fn waste_goes_back_to_stock() {
        let mut s = State::from_game(&Game::new(1, 3));
        let stock = s.pile(STOCK).to_vec();
        while !s.pile(STOCK).is_empty() {
            s = s.apply(&Mv { card_index: 1, from: STOCK, to: WASTE, out: false, pri: 0 });
        }
        assert!(s.pile(WASTE).iter().all(|&c| !down(c)));
        s = s.apply(&Mv { card_index: 0, from: WASTE, to: STOCK, out: false, pri: 0 });
        assert!(s.pile(WASTE).is_empty());
        assert_eq!(s.pile(STOCK), &stock[..]);
    }

    #[test]
    fn suits_round_trip() {
        for s in Suit::ALL {
            assert_eq!(from_ps_suit(ps_suit(s) as usize), s);
        }
    }
}
