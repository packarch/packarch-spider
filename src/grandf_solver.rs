//! Outil de résolution de Grand-père : portage de `GrandfSolver` de
//! KPatience (© 2006-2009 Stephan Kulow, GPL-2.0-or-later) sur le moteur
//! `patsolve`.
//!
//! Comme l'original, il connaît les cartes face cachée et joue d'office
//! toute sortie possible ; la redistribution est un coup comme un autre (de
//! priorité basse), qui change de « grappe » (explorée tout de suite).
//! KPatience garde dans la position les colonnes telles qu'elles étaient à
//! chaque redistribution : ici, un hachage de cet historique.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::Suit;
use crate::grandf::{DEALS, FOUNDATION, Game, PILES, STORES};
use crate::patsolve::{self, DOWN, Piles};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang.
const ACE: u8 = 1;
const KING: u8 = 13;

fn rank(c: u8) -> u8 {
    c & 0xF
}
fn suit(c: u8) -> usize {
    ((c >> 4) & 3) as usize
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

/// Un coup de patsolve (`MOVE`) ; `redeal` : la redistribution.
#[derive(Clone, Copy, Debug)]
struct Mv {
    /// Nombre de cartes au-dessus de celle qui part (0 : celle du dessus).
    card_index: usize,
    from: usize,
    to: usize,
    out: bool,
    redeal: bool,
    pri: i32,
}

/// Une position, sans allocation.
#[derive(Clone, Copy)]
struct State {
    w: Piles<STORES>,
    /// Hauteur des fondations, par couleur (codage patsolve).
    o: [u8; 4],
    /// Pile de fondation du jeu attribuée à chaque couleur ; 0 : aucune.
    slot: [u8; 4],
    /// Redistributions faites (`m_redeal`, 0 à 2).
    redeals: u8,
    /// Hachage des colonnes au moment de chaque redistribution.
    history: u64,
}

/// `GrandfSolver::make_move` (redistribution) : les cartes ramassées de la
/// 7e colonne à la 1re, distribuées en serpentin comme `Grandf::deal`.
fn redeal(w: &Piles<STORES>) -> Piles<STORES> {
    let mut deck: Vec<u8> = (0..STORES).rev().flat_map(|p| w.pile(p).iter().map(|&c| c & !DOWN)).collect();
    let mut piles: [Vec<u8>; STORES] = Default::default();
    let (mut start, mut stop, mut dir) = (0i32, STORES as i32 - 1, 1i32);
    for _ in 0..STORES {
        let mut i = start;
        loop {
            if let Some(c) = deck.pop() {
                piles[i as usize].push(if i == start { c } else { c | DOWN });
            }
            i += dir;
            if i == stop + dir {
                break;
            }
        }
        let t = start;
        start = stop;
        stop = t + dir;
        dir = -dir;
    }
    let mut j = 0;
    while let Some(c) = deck.pop() {
        piles[j + 1].push(c);
        j = (j + 1) % 6;
    }
    for p in &mut piles {
        if let Some(top) = p.last_mut() {
            *top &= !DOWN;
        }
    }
    Piles::new(piles)
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
        State { w, o, slot, redeals: g.deals - 1, history: 0 }
    }

    fn pile(&self, i: usize) -> &[u8] {
        self.w.pile(i)
    }

    fn outs(&self) -> i32 {
        self.o.iter().map(|&x| x as i32).sum()
    }

    fn is_won(&self) -> bool {
        self.o.iter().all(|&x| x == KING)
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        self.w.hash(&mut h);
        patsolve::fnv(&mut h, &self.o);
        patsolve::fnv(&mut h, &[self.redeals]);
        patsolve::fnv(&mut h, &self.history.to_le_bytes());
        h
    }

    /// La pile de fondation du jeu pour la couleur `o` : celle qui lui est
    /// attribuée, sinon la première libre (`GrandfSolver::translateMove`).
    fn foundation_pile(&self, o: usize) -> usize {
        if self.slot[o] != 0 {
            return self.slot[o] as usize;
        }
        (FOUNDATION..PILES).find(|&f| !self.slot.contains(&(f as u8))).expect("une fondation libre")
    }

    /// `GrandfSolver::make_move`, sur une copie.
    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        if m.redeal {
            let mut h = s.history ^ patsolve::FNV_START;
            s.w.hash(&mut h);
            s.history = h;
            s.w = redeal(&s.w);
            s.redeals += 1;
            return s;
        }
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
        if m.redeal {
            return Move::Deal;
        }
        let index = self.pile(m.from).len() - m.card_index - 1;
        let to = if m.out { self.foundation_pile(m.to) } else { m.to };
        Move::Cards { from: m.from, index, to }
    }

    /// `GrandfSolver::get_possible_moves` : une sortie possible est jouée
    /// d'office (seule) ; sinon les coups entre colonnes, puis la
    /// redistribution.
    fn possible_moves(&self) -> Vec<Mv> {
        let w: [&[u8]; STORES] = std::array::from_fn(|i| self.pile(i));
        for (i, pile) in w.iter().enumerate() {
            let Some(&card) = pile.last() else { continue };
            let o = suit(card);
            let empty = self.o[o] == 0;
            if (empty && rank(card) == ACE) || (!empty && rank(card) == self.o[o] + 1) {
                return vec![Mv { card_index: 0, from: i, to: o, out: true, redeal: false, pri: 127 }];
            }
        }
        let mut out = Vec::new();
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
                    let fits = w[j].last().is_some_and(|&top| rank(card) + 1 == rank(top) && suit(card) == suit(top));
                    // Un Roi vers une colonne vide ; même déjà au fond, tant
                    // qu'une redistribution reste (elle dépend des places).
                    let king = rank(card) == KING && w[j].is_empty() && (l != wlen - 1 || self.redeals < DEALS - 1);
                    if fits || king {
                        let turns = wlen > l + 1 && down(w[i][wlen - l - 2]);
                        let pri = if turns || wlen == l + 1 { 30 } else { 1 };
                        out.push(Mv { card_index: l, from: i, to: j, out: false, redeal: false, pri });
                    }
                }
            }
        }
        if self.redeals < DEALS - 1 {
            out.push(Mv { card_index: 0, from: 0, to: 0, out: false, redeal: true, pri: -1 });
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
    /// `GrandfSolver::getClusterNumber` : le nombre de redistributions.
    fn cluster(&self) -> u32 {
        self.redeals as u32
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
/// essaierait, sauf la redistribution), pour les conseils quand il ne
/// connaît pas de chemin gagnant.
pub fn useful_moves(game: &Game) -> Vec<Move> {
    let s = State::from_game(game);
    s.possible_moves().iter().filter(|m| !m.redeal).map(|m| s.game_move(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La redistribution du solveur est celle du jeu.
    #[test]
    fn redeal_matches_the_game() {
        let mut g = Game::new(7);
        let s = State::from_game(&g);
        assert!(g.play(Move::Deal));
        let t = s.apply(&Mv { card_index: 0, from: 0, to: 0, out: false, redeal: true, pri: -1 });
        let expected = State::from_game(&g);
        for i in 0..STORES {
            assert_eq!(t.pile(i), expected.pile(i), "colonne {i}");
        }
        assert_eq!(t.redeals, 1);
    }

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
