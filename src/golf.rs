//! Règles du Golf (comme dans KPatience, `golf.cpp`).
//!
//! - 52 cartes : 7 colonnes de 5 cartes visibles ; les 17 autres forment le
//!   talon, dont la première carte est retournée sur le rebut ;
//! - on pose la carte du dessus d'une colonne sur le rebut si son rang est
//!   immédiatement supérieur ou inférieur à celui du dessus du rebut (sans
//!   passer du Roi à l'As) ;
//! - un clic sur le talon retourne sa carte du dessus sur le rebut ; le
//!   talon ne se refait pas ;
//! - la partie est gagnée quand les colonnes sont vides : le dépôt
//!   automatique retourne alors le reste du talon (`Golf::drop`).
//!
//! Piles : colonnes 0 à 6, rebut 7, talon 8.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 7;
pub const WASTE: usize = 7;
pub const TALON: usize = 8;
pub const PILES: usize = 9;
/// Cartes par colonne au départ.
pub const DEPTH: usize = 5;

/// `Golf::checkAdd` : `card` peut-elle aller sur `top` ?
pub fn fits(card: u8, top: u8) -> bool {
    card + 1 == top || card == top + 1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `golf` : distingue une partie de Golf des autres jeux dans la partie
/// enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub golf: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Golf::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for _ in 0..DEPTH {
            for pile in piles.iter_mut().take(STORES) {
                pile.push(Card { face_up: true, ..list.pop().expect("paquet complet") });
            }
        }
        // Le reste va au talon dans l'ordre de la liste : la dernière carte
        // est sur le dessus ; elle est aussitôt retournée sur le rebut.
        piles[TALON] = list.into_iter().map(|c| Card { face_up: false, ..c }).collect();
        let first = piles[TALON].pop().expect("talon");
        piles[WASTE].push(Card { face_up: true, ..first });
        Game { golf: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[..STORES].iter().all(Vec::is_empty) && self.piles[TALON].is_empty()
    }

    /// Peut-on retourner une carte du talon ? (`Golf::newCards`)
    pub fn can_draw(&self) -> bool {
        !self.piles[TALON].is_empty()
    }

    /// `Golf::checkRemove` : seule la carte du dessus d'une colonne se prend.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES && index + 1 == self.piles[p].len()
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_draw(),
            Move::Cards { from, index, to } => {
                to == WASTE
                    && self.can_pick(from, index)
                    && self.piles[WASTE].last().is_some_and(|t| fits(self.piles[from][index].rank, t.rank))
            }
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        self.undo.push(Snapshot { piles: self.piles.clone(), moves: self.moves });
        self.redo.clear();
        let card = match m {
            Move::Deal => Card { face_up: true, ..self.piles[TALON].pop().expect("talon") },
            Move::Cards { from, .. } => self.piles[from].pop().expect("colonne"),
        };
        self.piles[WASTE].push(card);
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

    /// `Golf::drop` : les colonnes vides, le talon se retourne carte à carte.
    pub fn autodrop(&self) -> Option<Move> {
        (self.piles[..STORES].iter().all(Vec::is_empty) && self.can_draw()).then_some(Move::Deal)
    }

    /// Double-clic (ou clic, comme `tryAutomaticMove`) : la carte du dessus
    /// va sur le rebut si elle le peut.
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        let m = Move::Cards { from, index, to: WASTE };
        self.is_legal(m).then_some(m)
    }

    /// Les coups possibles, pour les conseils quand l'outil de résolution ne
    /// connaît pas de chemin gagnant.
    pub fn sensible_moves(&self) -> Vec<Move> {
        (0..STORES).filter_map(|p| self.piles[p].len().checked_sub(1).and_then(|i| self.auto_move(p, i))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_deal() {
        let g = Game::new(1);
        let counts: Vec<usize> = g.piles.iter().map(Vec::len).collect();
        assert_eq!(counts, vec![5, 5, 5, 5, 5, 5, 5, 1, 16]);
        assert!(g.piles[..=WASTE].iter().flatten().all(|c| c.face_up));
        assert!(g.piles[TALON].iter().all(|c| !c.face_up));
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn rules() {
        assert!(fits(5, 6) && fits(7, 6) && fits(12, 13) && fits(13, 12));
        assert!(!fits(1, 13) && !fits(13, 1) && !fits(6, 6));
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        let card = |rank, suit| Card { id: 0, rank, suit, face_up: true };
        g.piles[WASTE].push(card(8, Suit::Hearts));
        g.piles[0] = vec![card(2, Suit::Clubs), card(9, Suit::Spades)];
        g.piles[1] = vec![card(7, Suit::Clubs), card(3, Suit::Spades)];
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: WASTE }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 0, to: WASTE }));
        assert!(!g.is_legal(Move::Cards { from: 1, index: 1, to: WASTE }));
        assert_eq!(g.sensible_moves(), vec![Move::Cards { from: 0, index: 1, to: WASTE }]);
        assert!(!g.is_legal(Move::Deal));
        assert_eq!(g.autodrop(), None);
    }

    #[test]
    fn talon_is_flipped_once_columns_are_empty() {
        let mut g = Game::new(1);
        for p in g.piles[..STORES].iter_mut() {
            p.clear();
        }
        assert!(!g.is_won());
        while let Some(m) = g.autodrop() {
            assert!(g.play(m));
        }
        assert!(g.is_won());
        assert_eq!(g.piles[WASTE].len(), 17);
        assert!(g.undo());
        assert_eq!(g.piles[TALON].len(), 1);
    }
}
