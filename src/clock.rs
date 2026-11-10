//! Règles de l'Horloge du grand-père (comme dans KPatience, `clock.cpp`).
//!
//! - 52 cartes, toutes visibles ; 12 d'entre elles (9♦ 10♠ V♥ D♣ R♦ 2♠ 3♥
//!   4♣ 5♦ 6♠ 7♥ 8♣) forment le cadran : ce sont les fondations, d'1 heure
//!   à midi ; les 40 autres vont dans 8 piles de 5 ;
//! - les fondations montent dans leur couleur, et repassent du Roi à l'As ;
//!   la partie est gagnée quand les 8 piles sont vides ;
//! - seule la carte du dessus d'une pile se déplace : sur une fondation, sur
//!   une carte de rang immédiatement supérieur (couleur libre) ou sur une
//!   pile vide.
//!
//! Piles : les 8 piles 0 à 7, puis les 12 fondations 8 à 19 (`target[0]` à
//! `target[11]` de KPatience, 9♦ à 8♣).

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 8;
pub const FOUNDATION: usize = 8;
pub const HOURS: usize = 12;
pub const PILES: usize = STORES + HOURS;

/// Les cartes du cadran, dans l'ordre des fondations de KPatience.
pub const DIAL: [(u8, Suit); HOURS] = [
    (9, Suit::Diamonds),
    (10, Suit::Spades),
    (11, Suit::Hearts),
    (12, Suit::Clubs),
    (13, Suit::Diamonds),
    (2, Suit::Spades),
    (3, Suit::Hearts),
    (4, Suit::Clubs),
    (5, Suit::Diamonds),
    (6, Suit::Spades),
    (7, Suit::Hearts),
    (8, Suit::Clubs),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `clock` : distingue une partie de l'Horloge des autres jeux dans la
/// partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub clock: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Clock::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        let mut j = 0;
        while let Some(mut c) = list.pop() {
            c.face_up = true;
            match DIAL.iter().position(|&(r, s)| r == c.rank && s == c.suit) {
                Some(i) => piles[FOUNDATION + i].push(c),
                None => {
                    piles[j].push(c);
                    j = (j + 1) % STORES;
                }
            }
        }
        Game { clock: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[..STORES].iter().all(Vec::is_empty)
    }

    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES && index + 1 == self.piles[p].len()
    }

    /// `Clock::checkAdd`.
    pub fn can_drop(&self, from: usize, index: usize, to: usize) -> bool {
        let Some(card) = self.piles[from].get(index) else { return false };
        match self.piles[to].last() {
            None => to < STORES,
            Some(top) if to < STORES => card.rank + 1 == top.rank,
            Some(top) => top.suit == card.suit && (card.rank == top.rank + 1 || (top.rank == 13 && card.rank == 1)),
        }
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => false,
            Move::Cards { from, index, to } => {
                from != to && from < PILES && to < PILES && self.can_pick(from, index) && self.can_drop(from, index, to)
            }
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        let Move::Cards { from, index, to } = m else { return false };
        self.undo.push(Snapshot { piles: self.piles.clone(), moves: self.moves });
        self.redo.clear();
        let moving = self.piles[from].split_off(index);
        self.piles[to].extend(moving);
        self.moves += 1;
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                self.redo.push(Snapshot { piles: std::mem::replace(&mut self.piles, s.piles), moves: self.moves });
                self.moves = s.moves;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                self.undo.push(Snapshot { piles: std::mem::replace(&mut self.piles, s.piles), moves: self.moves });
                self.moves = s.moves;
                true
            }
            None => false,
        }
    }

    /// Double-clic : vers la fondation qui l'accepte, sinon sur une pile
    /// dont la carte est d'un rang au-dessus.
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        (FOUNDATION..PILES)
            .chain((0..STORES).filter(|&p| !self.piles[p].is_empty()))
            .find(|&to| to != from && self.can_drop(from, index, to))
            .map(|to| Move::Cards { from, index, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_deal() {
        let g = Game::new(1);
        assert!(g.piles[..STORES].iter().all(|p| p.len() == 5));
        for (i, &(r, s)) in DIAL.iter().enumerate() {
            let p = &g.piles[FOUNDATION + i];
            assert_eq!(p.len(), 1);
            assert_eq!((p[0].rank, p[0].suit), (r, s));
        }
        assert!(g.piles.iter().flatten().all(|c| c.face_up));
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        let c = |rank, suit| Card { id: 0, rank, suit, face_up: true };
        for p in g.piles[..STORES].iter_mut() {
            p.clear();
        }
        // R♦ (5 h) reçoit l'As de carreau.
        g.piles[0] = vec![c(1, Suit::Diamonds)];
        g.piles[1] = vec![c(10, Suit::Hearts)];
        g.piles[2] = vec![c(9, Suit::Clubs)];
        assert!(g.is_legal(Move::Cards { from: 0, index: 0, to: FOUNDATION + 4 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 0, to: FOUNDATION }));
        // 9 sur 10, couleur libre ; pile vide : n'importe quelle carte.
        assert!(g.is_legal(Move::Cards { from: 2, index: 0, to: 1 }));
        assert!(g.is_legal(Move::Cards { from: 2, index: 0, to: 3 }));
        assert!(!g.is_legal(Move::Cards { from: 1, index: 0, to: 2 }));
        assert_eq!(g.auto_move(0, 0), Some(Move::Cards { from: 0, index: 0, to: FOUNDATION + 4 }));
    }
}
