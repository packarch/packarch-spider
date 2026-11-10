//! Règles de Bizilizi, comme dans KPatience (`bizilizi.cpp`).
//!
//! - Un Klondike sans talon : 7 colonnes distribuées comme au Klondike, plus
//!   2 (ou 3) cartes cachées sous les premières colonnes : 23 (ou 24) cartes
//!   cachées en tout ;
//! - les cartes restantes forment la réserve, toutes visibles, rangées par
//!   rang du Roi au 2 (les As partent d'avance sur les fondations) ; on y
//!   prend n'importe quelle carte, une seule à la fois ;
//! - sur une colonne, on descend en alternant rouge et noir ; une colonne
//!   vide ne reçoit qu'un Roi (et ce qui le suit) ;
//! - 4 fondations, de l'As au Roi par couleur ; rien n'en redescend.
//!
//! Piles : colonnes 0 à 6, fondations 7 à 10, réserve 11.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 7;
pub const FOUNDATION: usize = 7;
pub const POOL: usize = 11;
pub const PILES: usize = 12;

/// Fondation préférée de chaque couleur (`Bizilizi::dealphaPos`) : carreau,
/// trèfle, cœur, pique, pour alterner les teintes.
pub fn preferred_foundation(s: Suit) -> usize {
    FOUNDATION
        + match s {
            Suit::Clubs => 1,
            Suit::Diamonds => 0,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        }
}

/// `alernatingCardHash` : ordre de la réserve (décroissant), qui fait
/// alterner les teintes à chaque rang.
fn pool_hash(c: &Card) -> u32 {
    const ALT_POS: [u32; 8] = [1, 3, 2, 0, 3, 1, 0, 2];
    c.rank as u32 * 4 + ALT_POS[(c.rank as usize % 2) * 4 + c.suit as usize]
}

/// `card` peut-elle aller sur `below` dans une colonne ?
pub fn builds_on(card: &Card, below: &Card) -> bool {
    card.rank + 1 == below.rank && card.suit.is_red() != below.suit.is_red()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `bizilizi` (2 ou 3 cartes cachées en plus) distingue une partie de
/// Bizilizi des autres jeux dans la partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub bizilizi: u8,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Bizilizi::restart`).
    /// `extra` : 2 (23 cartes cachées) ou 3 (24).
    pub fn new(extra: u8, deal_number: u32) -> Game {
        let extra = if extra == 3 { 3 } else { 2 };
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for pile in piles.iter_mut().take(extra as usize) {
            pile.push(Card { face_up: false, ..list.pop().expect("paquet complet") });
        }
        for round in 0..STORES {
            for (i, pile) in piles.iter_mut().enumerate().take(STORES).skip(round) {
                pile.push(Card { face_up: i == round, ..list.pop().expect("paquet complet") });
            }
        }
        list.sort_by_key(|c| std::cmp::Reverse(pool_hash(c)));
        for c in list {
            let c = Card { face_up: true, ..c };
            if c.rank == 1 {
                piles[preferred_foundation(c.suit)].push(c);
            } else {
                piles[POOL].push(c);
            }
        }
        Game { bizilizi: extra, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..POOL].iter().all(|p| p.len() == 13)
    }

    /// `Bizilizi::checkRemove` : une suite visible d'une colonne, ou une carte
    /// quelconque de la réserve.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        let pile = &self.piles[p];
        match p {
            0..STORES => {
                index < pile.len() && pile[index..].iter().all(|c| c.face_up) && pile[index..].windows(2).all(|w| builds_on(&w[1], &w[0]))
            }
            POOL => index < pile.len(),
            _ => false,
        }
    }

    /// Les cartes qui partent : une seule depuis la réserve.
    fn moving(&self, from: usize, index: usize) -> &[Card] {
        if from == POOL { &self.piles[from][index..=index] } else { &self.piles[from][index..] }
    }

    fn fits_foundation(&self, f: usize, card: &Card) -> bool {
        match self.piles[f].last() {
            None => card.rank == 1,
            Some(t) => t.suit == card.suit && t.rank + 1 == card.rank,
        }
    }

    /// La fondation qui prend `card` : celle de sa couleur, sinon la préférée
    /// pour un As, sinon la première vide.
    pub fn foundation_for(&self, card: &Card) -> Option<usize> {
        let fits = |f: &usize| self.fits_foundation(*f, card);
        (FOUNDATION..POOL)
            .filter(|&f| !self.piles[f].is_empty())
            .find(fits)
            .or_else(|| Some(preferred_foundation(card.suit)).filter(fits))
            .or_else(|| (FOUNDATION..POOL).find(fits))
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => false,
            Move::Cards { from, index, to } => {
                if from == to || to >= POOL || !self.can_pick(from, index) {
                    return false;
                }
                let moving = self.moving(from, index);
                let first = &moving[0];
                if to >= FOUNDATION {
                    moving.len() == 1 && (from == POOL || index + 1 == self.piles[from].len()) && self.fits_foundation(to, first)
                } else {
                    match self.piles[to].last() {
                        None => first.rank == 13,
                        Some(t) => builds_on(first, t),
                    }
                }
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
        if from == POOL {
            let c = self.piles[POOL].remove(index);
            self.piles[to].push(c);
        } else {
            let moving = self.piles[from].split_off(index);
            self.piles[to].extend(moving);
            if let Some(top) = self.piles[from].last_mut() {
                top.face_up = true;
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

    /// Hauteur de la fondation de la couleur `s` (0 si pas commencée).
    fn height(&self, s: Suit) -> u8 {
        self.piles[FOUNDATION..POOL].iter().filter_map(|p| p.last()).find(|t| t.suit == s).map_or(0, |t| t.rank)
    }

    /// `BiziliziSolver::good_automove` (règle de Raymond).
    pub fn good_automove(&self, card: &Card) -> bool {
        let r = card.rank;
        if r <= 2 {
            return true;
        }
        let red = card.suit.is_red();
        let opposite: Vec<u8> = Suit::ALL.into_iter().filter(|s| s.is_red() != red).map(|s| self.height(s)).collect();
        if opposite.iter().all(|&h| h + 1 >= r) {
            return true;
        }
        let same_other = Suit::ALL.into_iter().find(|&s| s != card.suit && s.is_red() == red).expect("deux couleurs par teinte");
        opposite.iter().all(|&h| h + 2 >= r) && self.height(same_other) + 3 >= r
    }

    /// Le dépôt automatique de KPatience (`DealerScene::drop`, sorties de
    /// priorité 127 de `BiziliziSolver`) : la première sortie sans risque,
    /// du dessus d'une colonne puis de la réserve.
    pub fn autodrop(&self) -> Option<Move> {
        let tops = (0..STORES).filter_map(|p| self.piles[p].len().checked_sub(1).map(|i| (p, i)));
        let pool = (0..self.piles[POOL].len()).map(|i| (POOL, i));
        tops.chain(pool).find_map(|(from, index)| {
            let card = &self.piles[from][index];
            let to = self.foundation_for(card)?;
            self.good_automove(card).then_some(Move::Cards { from, index, to })
        })
    }

    /// Double-clic (`Bizilizi::tryAutomaticMove`) : vers une fondation, sinon
    /// vers une colonne (une carte plutôt qu'une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let mv = |to| Move::Cards { from, index, to };
        if (from == POOL || index + 1 == self.piles[from].len())
            && let Some(to) = self.foundation_for(&self.piles[from][index])
        {
            return Some(mv(to));
        }
        let legal = |to: &usize| self.is_legal(mv(*to));
        (0..STORES)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..STORES).filter(legal).find(|_| from == POOL || index > 0))
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
        for (extra, hidden) in [(2u8, 23usize), (3, 24)] {
            let g = Game::new(extra, 1);
            let cols = &g.piles[..STORES];
            assert_eq!(cols.iter().flatten().filter(|c| !c.face_up).count(), hidden);
            assert!(cols.iter().all(|p| p.last().unwrap().face_up && p.iter().filter(|c| c.face_up).count() == 1));
            let aces: usize = g.piles[FOUNDATION..POOL].iter().map(Vec::len).sum();
            assert_eq!(g.piles[POOL].len() + aces, 52 - 28 - extra as usize);
            assert!(g.piles[POOL].iter().all(|c| c.face_up && c.rank != 1));
            for f in FOUNDATION..POOL {
                if let Some(a) = g.piles[f].first() {
                    assert_eq!(preferred_foundation(a.suit), f);
                }
            }
            // La réserve, du Roi au 2.
            assert!(g.piles[POOL].windows(2).all(|w| w[0].rank >= w[1].rank));
            let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
            ids.sort();
            assert_eq!(ids, (0..52).collect::<Vec<u8>>());
        }
        // Comme KPatience : les cartes cachées en plus sont les dernières de
        // la liste, sur les colonnes 0 et 1.
        let list = cards::shuffled(&cards::deck(1, &Suit::ALL), 1);
        let g = Game::new(2, 1);
        assert_eq!(g.piles[0][0].id, list[51].id);
        assert_eq!(g.piles[1][0].id, list[50].id);
        assert_eq!(g.piles[0][1].id, list[49].id);
    }

    #[test]
    fn rules() {
        let mut g = Game::new(2, 1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(5, Suit::Clubs, false), card(9, Suit::Hearts, true), card(8, Suit::Spades, true)];
        g.piles[1] = vec![card(10, Suit::Clubs, true)];
        g.piles[POOL] = vec![card(13, Suit::Hearts, true), card(7, Suit::Hearts, true), card(2, Suit::Spades, true)];
        g.piles[FOUNDATION + 3] = vec![card(1, Suit::Spades, true)];
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(!g.can_pick(0, 0));
        // N'importe quelle carte de la réserve, seule.
        assert!(g.is_legal(Move::Cards { from: POOL, index: 1, to: 0 }));
        assert!(g.is_legal(Move::Cards { from: POOL, index: 0, to: 2 }), "un Roi sur une colonne vide");
        assert!(!g.is_legal(Move::Cards { from: POOL, index: 1, to: 2 }));
        assert!(g.is_legal(Move::Cards { from: POOL, index: 2, to: FOUNDATION + 3 }));
        assert!(g.play(Move::Cards { from: POOL, index: 1, to: 0 }));
        assert_eq!(g.piles[POOL].len(), 2);
        assert_eq!(g.piles[0].last().unwrap().rank, 7);
        // Rien ne redescend des fondations.
        assert!(!g.can_pick(FOUNDATION + 3, 0));
        assert_eq!(g.autodrop(), Some(Move::Cards { from: POOL, index: 1, to: FOUNDATION + 3 }));
    }
}
