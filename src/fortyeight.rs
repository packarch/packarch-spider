//! Règles de Quarante & Huit (Forty & Eight), comme dans KPatience
//! (`fortyeight.cpp`).
//!
//! - 104 cartes (2 jeux) : 8 colonnes de 4 cartes visibles ; le reste au
//!   talon, dont la première carte est retournée sur le rebut ;
//! - 8 fondations, montées de l'As au Roi dans la même couleur ;
//! - sur une colonne, on descend dans la même couleur ; une colonne vide
//!   reçoit n'importe quelle carte ; une suite de même couleur se déplace
//!   d'un bloc si elle compte au plus 2^(colonnes vides) cartes (sans
//!   compter la colonne d'arrivée si elle est vide) ;
//! - seule la carte du dessus du rebut se joue ;
//! - un clic sur le talon retourne une carte sur le rebut ; talon vide, le
//!   rebut se retourne en talon, une seule fois.
//!
//! Piles : colonnes 0 à 7, fondations 8 à 15, rebut 16, talon 17.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 8;
pub const FOUNDATION: usize = 8;
pub const WASTE: usize = 16;
pub const TALON: usize = 17;
pub const PILES: usize = 18;
/// Cartes par colonne au départ.
pub const DEPTH: usize = 4;

/// `isSameSuitDescending` : chaque carte est d'un rang sous la précédente,
/// dans la même couleur.
pub fn is_run(cards: &[Card]) -> bool {
    cards.windows(2).all(|w| w[1].suit == w[0].suit && w[1].rank + 1 == w[0].rank)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    lastdeal: bool,
    moves: u32,
}

/// `fortyeight` : distingue une partie de Quarante & Huit des autres jeux
/// dans la partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub fortyeight: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    /// Le rebut a déjà été retourné en talon (`Fortyeight::lastdeal`).
    pub lastdeal: bool,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience
    /// (`Fortyeight::restart`) : 4 rangées prises à la fin de la liste,
    /// colonne 0 d'abord ; le reste au talon dans l'ordre de la liste (la
    /// dernière carte sur le dessus), puis une carte sur le rebut.
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(2, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for _ in 0..DEPTH {
            for pile in piles.iter_mut().take(STORES) {
                pile.push(Card { face_up: true, ..list.pop().expect("paquet complet") });
            }
        }
        piles[TALON] = list.into_iter().map(|c| Card { face_up: false, ..c }).collect();
        let first = piles[TALON].pop().expect("talon");
        piles[WASTE].push(Card { face_up: true, ..first });
        Game { fortyeight: true, deal_number, piles, lastdeal: false, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..WASTE].iter().all(|p| p.len() == 13)
    }

    /// Peut-on piocher ou retourner le rebut ? (`Fortyeight::newCards`)
    pub fn can_deal(&self) -> bool {
        !self.piles[TALON].is_empty() || !self.lastdeal
    }

    fn free_columns(&self) -> usize {
        self.piles[..STORES].iter().filter(|p| p.is_empty()).count()
    }

    /// `Fortyeight::checkRemove`.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        let pile = &self.piles[p];
        match p {
            0..STORES => index < pile.len() && is_run(&pile[index..]),
            WASTE => index + 1 == pile.len(),
            _ => false,
        }
    }

    /// `checkAddSameSuitAscendingFromAce` pour une carte.
    pub fn fits_foundation(&self, f: usize, card: &Card) -> bool {
        match self.piles[f].last() {
            None => card.rank == 1,
            Some(t) => t.suit == card.suit && t.rank + 1 == card.rank,
        }
    }

    /// `Fortyeight::canPutStore` : `count` cartes, de première carte `card`,
    /// sur la colonne `to`.
    fn fits_store(&self, to: usize, card: &Card, count: usize) -> bool {
        let to_empty = self.piles[to].is_empty();
        let frees = self.free_columns() - usize::from(to_empty);
        if count > 1 << frees {
            return false;
        }
        self.piles[to].last().is_none_or(|t| t.suit == card.suit && t.rank == card.rank + 1)
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_deal(),
            Move::Cards { from, index, to } => {
                if from == to || to >= WASTE || !self.can_pick(from, index) {
                    return false;
                }
                let card = &self.piles[from][index];
                let count = self.piles[from].len() - index;
                if to >= FOUNDATION { count == 1 && self.fits_foundation(to, card) } else { self.fits_store(to, card, count) }
            }
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        self.undo.push(Snapshot { piles: self.piles.clone(), lastdeal: self.lastdeal, moves: self.moves });
        self.redo.clear();
        match m {
            Move::Deal => {
                if let Some(c) = self.piles[TALON].pop() {
                    self.piles[WASTE].push(Card { face_up: true, ..c });
                } else {
                    // Le rebut retourné : sa première carte revient sur le
                    // dessus du talon.
                    self.lastdeal = true;
                    let waste = std::mem::take(&mut self.piles[WASTE]);
                    self.piles[TALON] = waste.into_iter().rev().map(|c| Card { face_up: false, ..c }).collect();
                }
            }
            Move::Cards { from, index, to } => {
                let cards = self.piles[from].split_off(index);
                self.piles[to].extend(cards);
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

    fn snapshot(&self) -> Snapshot {
        Snapshot { piles: self.piles.clone(), lastdeal: self.lastdeal, moves: self.moves }
    }

    fn restore(&mut self, s: Snapshot) {
        self.piles = s.piles;
        self.lastdeal = s.lastdeal;
        self.moves = s.moves;
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                self.redo.push(self.snapshot());
                self.restore(s);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                self.undo.push(self.snapshot());
                self.restore(s);
                true
            }
            None => false,
        }
    }

    /// La fondation qui prend `card` (la première, comme
    /// `FortyeightSolver::translateMove`).
    pub fn foundation_for(&self, card: &Card) -> Option<usize> {
        (FOUNDATION..WASTE).find(|&f| self.fits_foundation(f, card))
    }

    /// Le dépôt automatique de KPatience (`DealerScene::drop`, sorties de
    /// priorité supérieure à 120 de `FortyeightSolver`) : toute carte qui
    /// peut sortir, d'abord du dessus d'une colonne, puis du rebut.
    pub fn autodrop(&self) -> Option<Move> {
        (0..STORES).chain([WASTE]).find_map(|from| {
            let index = self.piles[from].len().checked_sub(1)?;
            let to = self.foundation_for(&self.piles[from][index])?;
            Some(Move::Cards { from, index, to })
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
        if index + 1 == self.piles[from].len()
            && let Some(to) = self.foundation_for(&self.piles[from][index])
        {
            return Some(mv(to));
        }
        let legal = |to: &usize| self.is_legal(mv(*to));
        (0..STORES)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..STORES).filter(legal).find(|_| from == WASTE || index > 0))
            .map(mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: u8, suit: Suit) -> Card {
        Card { id: 0, rank, suit, face_up: true }
    }

    #[test]
    fn initial_deal() {
        let g = Game::new(1);
        let counts: Vec<usize> = g.piles.iter().map(Vec::len).collect();
        assert_eq!(counts, [vec![4; 8], vec![0; 8], vec![1, 71]].concat());
        assert!(g.piles[..=WASTE].iter().flatten().all(|c| c.face_up));
        assert!(g.piles[TALON].iter().all(|c| !c.face_up));
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..104).collect::<Vec<u8>>());
        // Comme KPatience : la colonne 0 reçoit la dernière carte de la
        // liste ; le talon garde l'ordre de la liste, rebut en premier.
        let list = cards::shuffled(&cards::deck(2, &Suit::ALL), 1);
        assert_eq!(g.piles[0][0].id, list[103].id);
        assert_eq!(g.piles[7][3].id, list[72].id);
        assert_eq!(g.piles[WASTE][0].id, list[71].id);
        assert_eq!(g.piles[TALON][0].id, list[0].id);
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(2, Suit::Clubs), card(9, Suit::Hearts), card(8, Suit::Hearts), card(7, Suit::Hearts)];
        g.piles[1] = vec![card(10, Suit::Hearts)];
        g.piles[2] = vec![card(9, Suit::Hearts)];
        for p in 3..STORES {
            g.piles[p] = vec![card(13, Suit::Diamonds)];
        }
        g.piles[WASTE] = vec![card(5, Suit::Clubs), card(1, Suit::Spades)];
        // Aucune colonne vide : une seule carte à la fois.
        assert!(g.can_pick(0, 1) && !g.can_pick(0, 0));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 3, to: 1 }));
        assert!(g.is_legal(Move::Cards { from: WASTE, index: 1, to: FOUNDATION + 3 }));
        assert!(!g.is_legal(Move::Cards { from: WASTE, index: 0, to: FOUNDATION }));
        assert_eq!(g.autodrop(), Some(Move::Cards { from: WASTE, index: 1, to: FOUNDATION }));
        // Une colonne vide : 2 cartes (1 vers la colonne vide elle-même).
        g.piles[7].clear();
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(g.is_legal(Move::Cards { from: 0, index: 2, to: 2 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 2, to: 7 }));
        assert!(g.is_legal(Move::Cards { from: 0, index: 3, to: 7 }));
        // Deux colonnes vides : 4 cartes (2 vers une colonne vide).
        g.piles[6].clear();
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 7 }));
        assert!(g.is_legal(Move::Cards { from: 0, index: 2, to: 7 }));
        assert_eq!(g.auto_move(0, 1), Some(Move::Cards { from: 0, index: 1, to: 1 }));
    }

    #[test]
    fn talon_redeals_once() {
        let mut g = Game::new(2);
        let first_waste = g.piles[WASTE][0].id;
        while !g.piles[TALON].is_empty() {
            assert!(g.play(Move::Deal));
        }
        assert_eq!(g.piles[WASTE].len(), 72);
        assert!(g.play(Move::Deal));
        assert!(g.lastdeal && g.piles[WASTE].is_empty());
        assert_eq!(g.piles[TALON].last().map(|c| c.id), Some(first_waste));
        assert!(g.piles[TALON].iter().all(|c| !c.face_up));
        assert!(g.undo());
        assert!(!g.lastdeal);
        assert!(g.redo());
        while !g.piles[TALON].is_empty() {
            assert!(g.play(Move::Deal));
        }
        assert!(!g.can_deal());
        assert!(!g.play(Move::Deal));
    }

    #[test]
    fn won() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        for (i, f) in (FOUNDATION..WASTE).enumerate() {
            g.piles[f] = (1..=13).map(|r| card(r, Suit::ALL[i / 2])).collect();
        }
        assert!(g.is_won());
    }
}
