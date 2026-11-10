//! Règles de Grand-père (Grandfather, comme dans KPatience, `grandf.cpp`).
//!
//! - 52 cartes en 7 colonnes, distribuées en serpentin (aller vers la droite,
//!   retour vers la gauche, chaque passage plus court) : la première carte de
//!   chaque passage est visible, les autres cachées ; le reste va, visible,
//!   sur les colonnes 2 à 7 ; enfin la carte du dessus de chaque colonne se
//!   retourne ;
//! - on pose une carte sur la carte de rang immédiatement supérieur **de la
//!   même couleur** ; une colonne vide ne reçoit qu'un Roi ;
//! - n'importe quelle carte visible se déplace avec toutes celles qui sont
//!   dessus ; la carte découverte se retourne ;
//! - 4 fondations, de l'As au Roi par couleur, sans retour ;
//! - deux fois par partie, on peut redistribuer : les colonnes sont ramassées
//!   (de la 7e à la 1re) et distribuées à nouveau de la même façon.
//!
//! Piles : colonnes 0 à 6, fondations 7 à 10.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 7;
pub const FOUNDATION: usize = 7;
pub const PILES: usize = 11;
/// Distributions par partie : la première et deux redistributions.
pub const DEALS: u8 = 3;

/// `Grandf::deal` : distribue `cards` (prises par la fin) sur les 7 colonnes.
pub fn deal(mut cards: Vec<Card>) -> Vec<Vec<Card>> {
    let mut piles: Vec<Vec<Card>> = vec![Vec::new(); STORES];
    let (mut start, mut stop, mut dir) = (0i32, STORES as i32 - 1, 1i32);
    for _ in 0..STORES {
        let mut i = start;
        loop {
            if let Some(c) = cards.pop() {
                piles[i as usize].push(Card { face_up: i == start, ..c });
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
    let mut i = 0;
    while let Some(c) = cards.pop() {
        piles[i + 1].push(Card { face_up: true, ..c });
        i = (i + 1) % 6;
    }
    for p in &mut piles {
        if let Some(top) = p.last_mut() {
            top.face_up = true;
        }
    }
    piles
}

/// `Grandf::newCards` : les cartes des colonnes, ramassées de la 7e à la 1re
/// (chacune du fond vers le dessus).
pub fn collect(stores: &[Vec<Card>]) -> Vec<Card> {
    stores.iter().rev().flatten().copied().collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    deals: u8,
    moves: u32,
}

/// `grandfather` : distingue une partie de Grand-père des autres jeux dans la
/// partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub grandfather: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    /// Distributions faites (1 à 3).
    pub deals: u8,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// `isSameSuitAscending`.
fn same_suit_ascending(cards: &[Card]) -> bool {
    cards.windows(2).all(|w| w[1].suit == w[0].suit && w[1].rank == w[0].rank + 1)
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Grandf::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut piles = deal(cards::shuffled(&deck, deal_number));
        piles.resize(PILES, Vec::new());
        Game { grandfather: true, deal_number, piles, deals: 1, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..].iter().map(Vec::len).sum::<usize>() == 52
    }

    /// Reste-t-il une redistribution ?
    pub fn can_redeal(&self) -> bool {
        self.deals < DEALS
    }

    /// `Grandf::checkRemove` : toute carte visible d'une colonne, avec celles
    /// qui sont dessus.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES && self.piles[p].get(index).is_some_and(|c| c.face_up)
    }

    /// `Grandf::checkAdd`.
    pub fn can_drop(&self, from: usize, index: usize, to: usize) -> bool {
        let moving = &self.piles[from][index..];
        let Some(first) = moving.first() else { return false };
        let top = self.piles[to].last();
        if to < STORES {
            match top {
                None => first.rank == 13,
                Some(t) => first.rank + 1 == t.rank && first.suit == t.suit,
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
            Move::Deal => self.can_redeal(),
            Move::Cards { from, index, to } => {
                from != to && from < PILES && to < PILES && self.can_pick(from, index) && self.can_drop(from, index, to)
            }
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        self.undo.push(Snapshot { piles: self.piles.clone(), deals: self.deals, moves: self.moves });
        self.redo.clear();
        match m {
            Move::Deal => {
                let cards = collect(&self.piles[..STORES]);
                for (pile, new) in self.piles.iter_mut().zip(deal(cards)) {
                    *pile = new;
                }
                self.deals += 1;
            }
            Move::Cards { from, index, to } => {
                let moving = self.piles[from].split_off(index);
                self.piles[to].extend(moving);
                // `setAutoTurnTop` : la carte découverte se retourne.
                if let Some(top) = self.piles[from].last_mut() {
                    top.face_up = true;
                }
            }
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
                self.redo.push(Snapshot { piles: std::mem::replace(&mut self.piles, s.piles), deals: self.deals, moves: self.moves });
                self.deals = s.deals;
                self.moves = s.moves;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                self.undo.push(Snapshot { piles: std::mem::replace(&mut self.piles, s.piles), deals: self.deals, moves: self.moves });
                self.deals = s.deals;
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

    /// Le dépôt automatique de KPatience (`DealerScene::drop`) : l'outil de
    /// résolution de Grand-père joue d'office toute sortie possible.
    pub fn autodrop(&self) -> Option<Move> {
        (0..STORES).find_map(|from| {
            let pile = &self.piles[from];
            let card = pile.last().filter(|c| c.face_up)?;
            let to = self.foundation_for(card)?;
            Some(Move::Cards { from, index: pile.len() - 1, to })
        })
    }

    /// Double-clic : la carte du dessus va sur une fondation ; sinon la carte
    /// (et celles qui sont dessus) va sur une colonne, une carte plutôt
    /// qu'une colonne vide.
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let mv = |to| Move::Cards { from, index, to };
        if let Some(to) = self.foundation_for(&self.piles[from][index])
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
        // Passages : 0→6, 6→1, 1→5, 5→2, 2→4, 4→3, 3 : 1, 3, 5, 7, 6, 4, 2
        // cartes ; les 24 autres, 4 par colonne de 2 à 7.
        assert_eq!(counts, vec![1, 7, 9, 11, 10, 8, 6]);
        for p in &g.piles[..STORES] {
            assert!(p.last().unwrap().face_up);
        }
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn redeal_twice_then_never() {
        let mut g = Game::new(5);
        assert!(g.play(Move::Deal));
        assert!(g.play(Move::Deal));
        assert!(!g.can_redeal());
        assert!(!g.play(Move::Deal));
        assert_eq!(g.piles[..STORES].iter().map(Vec::len).sum::<usize>(), 52);
        assert!(g.undo());
        assert_eq!(g.deals, 2);
        assert!(g.can_redeal());
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(4, Suit::Clubs, false), card(9, Suit::Hearts, true), card(2, Suit::Spades, true)];
        g.piles[1] = vec![card(10, Suit::Hearts, true)];
        g.piles[2] = vec![card(10, Suit::Diamonds, true)];
        // Même couleur seulement ; les cartes du dessus suivent.
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 2 }));
        assert!(g.play(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(g.piles[0][0].face_up, "la carte découverte se retourne");
        // Une colonne vide ne reçoit qu'un Roi.
        g.piles[3] = vec![card(13, Suit::Clubs, true)];
        g.piles[4].clear();
        assert!(!g.is_legal(Move::Cards { from: 2, index: 0, to: 4 }));
        assert!(g.is_legal(Move::Cards { from: 3, index: 0, to: 4 }));
        // Toute sortie possible est déposée d'office.
        g.piles[5] = vec![card(1, Suit::Hearts, true)];
        assert_eq!(g.autodrop(), Some(Move::Cards { from: 5, index: 0, to: FOUNDATION }));
    }

    #[test]
    fn collect_keeps_kpatience_order() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(1, Suit::Clubs, true), card(2, Suit::Clubs, true)];
        g.piles[6] = vec![card(3, Suit::Clubs, true)];
        let ranks: Vec<u8> = collect(&g.piles[..STORES]).iter().map(|c| c.rank).collect();
        assert_eq!(ranks, vec![3, 1, 2]);
    }
}
