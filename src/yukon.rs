//! Règles du Yukon (comme dans KPatience, `yukon.cpp`).
//!
//! - 52 cartes en 7 colonnes : 1 carte dans la première ; dans la colonne j,
//!   j cartes face cachée sous 5 cartes visibles ; pas de talon ;
//! - on pose une carte sur une carte de rang immédiatement supérieur et de
//!   l'autre teinte (rouge sur noir, noir sur rouge) ; une colonne vide ne
//!   reçoit qu'un Roi ;
//! - n'importe quelle carte visible se déplace, avec toutes celles qui sont
//!   dessus, même si elles ne se suivent pas ; la carte découverte se
//!   retourne ;
//! - 4 fondations, de l'As au Roi par couleur ; on ne reprend pas une carte
//!   d'une fondation.
//!
//! Piles : colonnes 0 à 6, fondations 7 à 10.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 7;
pub const FOUNDATION: usize = 7;
pub const PILES: usize = 11;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `yukon` : distingue une partie de Yukon des autres jeux dans la partie
/// enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub yukon: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// `isSameSuitAscending`.
fn same_suit_ascending(cards: &[Card]) -> bool {
    cards.windows(2).all(|w| w[1].suit == w[0].suit && w[1].rank == w[0].rank + 1)
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Yukon::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for round in 0..11 {
            for (j, pile) in piles.iter_mut().enumerate().take(STORES) {
                if (j == 0 && round == 0) || (j > 0 && round < j + 5) {
                    let c = list.pop().expect("paquet complet");
                    pile.push(Card { face_up: round >= j || j == 0, ..c });
                }
            }
        }
        Game { yukon: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..].iter().map(Vec::len).sum::<usize>() == 52
    }

    /// `Yukon::checkRemove` : toute carte visible d'une colonne, avec celles
    /// qui sont dessus.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES && self.piles[p].get(index).is_some_and(|c| c.face_up)
    }

    /// `Yukon::checkAdd`.
    pub fn can_drop(&self, from: usize, index: usize, to: usize) -> bool {
        let moving = &self.piles[from][index..];
        let Some(first) = moving.first() else { return false };
        let top = self.piles[to].last();
        if to < STORES {
            match top {
                None => first.rank == 13,
                Some(t) => first.rank + 1 == t.rank && first.suit.is_red() != t.suit.is_red(),
            }
        } else {
            same_suit_ascending(moving)
                && match top {
                    None => first.rank == 1,
                    Some(t) => t.suit == first.suit && t.rank + 1 == first.rank,
                }
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
        // `setAutoTurnTop` : la carte découverte se retourne.
        if let Some(top) = self.piles[from].last_mut() {
            top.face_up = true;
        }
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

    /// La fondation qui accepte `card` : celle de sa couleur, ou la première
    /// vide pour un As.
    fn foundation_for(&self, card: &Card) -> Option<usize> {
        (FOUNDATION..PILES).find(|&f| match self.piles[f].last() {
            None => card.rank == 1,
            Some(t) => t.suit == card.suit && t.rank + 1 == card.rank,
        })
    }

    /// Hauteur de la fondation de la couleur `suit`.
    fn out(&self, suit: Suit) -> u8 {
        self.piles[FOUNDATION..].iter().filter_map(|p| p.last()).find(|t| t.suit == suit).map_or(0, |t| t.rank)
    }

    /// Sortie sans risque (`YukonSolver::good_automove`, règle de Raymond).
    fn good_automove(&self, card: &Card) -> bool {
        let r = card.rank;
        if r <= 2 {
            return true;
        }
        let red = card.suit.is_red();
        let opposite: Vec<Suit> = Suit::ALL.into_iter().filter(|s| s.is_red() != red).collect();
        let same_other = Suit::ALL.into_iter().find(|s| *s != card.suit && s.is_red() == red).expect("deux couleurs par teinte");
        if opposite.iter().all(|&s| self.out(s) + 1 >= r) {
            return true;
        }
        opposite.iter().all(|&s| self.out(s) + 2 >= r) && self.out(same_other) + 3 >= r
    }

    /// Le dépôt automatique de KPatience (`DealerScene::drop`) : la première
    /// sortie sans risque, depuis le dessus d'une colonne.
    pub fn autodrop(&self) -> Option<Move> {
        (0..STORES).find_map(|from| {
            let pile = &self.piles[from];
            let card = pile.last().filter(|c| c.face_up)?;
            let to = self.foundation_for(card)?;
            self.good_automove(card).then(|| Move::Cards { from, index: pile.len() - 1, to })
        })
    }

    /// Double-clic : la carte du dessus va sur une fondation ; sinon la carte
    /// (et celles qui sont dessus) va sur une colonne, une carte plutôt
    /// qu'une colonne vide (et pas d'une colonne vers une autre vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let pile = &self.piles[from];
        let mv = |to| Move::Cards { from, index, to };
        if let Some(to) = self.foundation_for(&pile[index])
            && self.can_drop(from, index, to)
        {
            return Some(mv(to));
        }
        let legal = |to: &usize| *to != from && self.can_drop(from, index, *to);
        (0..STORES)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..STORES).filter(legal).find(|_| index > 0))
            .map(mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: u8, suit: Suit, face_up: bool) -> Card {
        Card { id: 0, rank, suit, face_up }
    }

    #[test]
    fn initial_deal() {
        let g = Game::new(1);
        let counts: Vec<usize> = g.piles[..STORES].iter().map(Vec::len).collect();
        assert_eq!(counts, vec![1, 6, 7, 8, 9, 10, 11]);
        for (j, p) in g.piles[..STORES].iter().enumerate() {
            let down = p.iter().filter(|c| !c.face_up).count();
            assert_eq!(down, j, "colonne {j}");
            assert!(p[down..].iter().all(|c| c.face_up));
        }
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(4, Suit::Clubs, false), card(9, Suit::Hearts, true), card(2, Suit::Spades, true), card(11, Suit::Diamonds, true)];
        g.piles[1] = vec![card(10, Suit::Spades, true)];
        g.piles[2] = vec![card(10, Suit::Hearts, true)];
        // Le 9♥ part avec les cartes qui sont dessus, même désordonnées.
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        // Pas sur la même teinte ; pas de carte cachée.
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 2 }));
        assert!(!g.can_pick(0, 0));
        assert!(g.play(Move::Cards { from: 0, index: 1, to: 1 }));
        assert_eq!(g.piles[1].len(), 4);
        assert!(g.piles[0][0].face_up, "la carte découverte se retourne");
        // Une colonne vide ne reçoit qu'un Roi.
        g.piles[3].clear();
        assert!(!g.is_legal(Move::Cards { from: 2, index: 0, to: 3 }));
        g.piles[4] = vec![card(13, Suit::Clubs, true)];
        assert!(g.is_legal(Move::Cards { from: 4, index: 0, to: 3 }));
        // Fondations : de l'As, même couleur, et pas de retour.
        g.piles[5] = vec![card(1, Suit::Hearts, true), card(2, Suit::Hearts, true)];
        assert!(!g.is_legal(Move::Cards { from: 5, index: 1, to: FOUNDATION }));
        // As et 2 de cœur ensemble : ils montent dans la même couleur.
        assert!(g.is_legal(Move::Cards { from: 5, index: 0, to: FOUNDATION }));
        assert!(g.play(Move::Cards { from: 5, index: 0, to: FOUNDATION }));
        assert!(!g.can_pick(FOUNDATION, 1));
    }

    #[test]
    fn autodrop_follows_raymonds_rule() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(1, Suit::Spades, true)];
        assert_eq!(g.autodrop(), Some(Move::Cards { from: 0, index: 0, to: FOUNDATION }));
        // Un 5 de cœur ne sort pas tant que les 4 noirs ne sont pas sortis.
        g.piles[0] = vec![card(5, Suit::Hearts, true)];
        g.piles[FOUNDATION] = (1..=4).map(|r| card(r, Suit::Hearts, true)).collect();
        assert_eq!(g.autodrop(), None);
        g.piles[FOUNDATION + 1] = (1..=4).map(|r| card(r, Suit::Clubs, true)).collect();
        g.piles[FOUNDATION + 2] = (1..=4).map(|r| card(r, Suit::Spades, true)).collect();
        assert_eq!(g.autodrop(), Some(Move::Cards { from: 0, index: 0, to: FOUNDATION }));
    }
}
