//! Règles de Simon le simple (comme dans KPatience, `simon.cpp`).
//!
//! - 52 cartes, toutes visibles, en 10 colonnes de 8, 8, 8, 7, 6, 5, 4, 3, 2
//!   et 1 carte ;
//! - on pose une carte sur une carte de rang immédiatement supérieur, de
//!   n'importe quelle couleur, ou sur une colonne vide ;
//! - on déplace une suite descendante (couleurs mêlées) si elle compte au
//!   plus 2^(colonnes vides) tronçons de même couleur : les colonnes vides
//!   servent d'appui, comme les « super-coups » du FreeCell ;
//! - seule une suite complète Roi → As de même couleur monte sur une
//!   fondation ; la partie est gagnée avec les 4.
//!
//! Piles : colonnes 0 à 9, fondations 10 à 13.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};
use crate::freecell::{Code, rank, suit};

pub const STORES: usize = 10;
pub const FOUNDATION: usize = 10;
pub const PILES: usize = 14;

/// `countSameSuitDescendingSequences` : le nombre de tronçons de même
/// couleur d'une suite descendante, `None` si elle ne descend pas.
pub fn sequences<C: Code>(cards: &[C]) -> Option<usize> {
    if cards.len() <= 1 {
        return Some(0);
    }
    let mut count = 1;
    for w in cards.windows(2) {
        let (a, b) = (w[0].code(), w[1].code());
        if rank(b) + 1 != rank(a) {
            return None;
        }
        if suit(b) != suit(a) {
            count += 1;
        }
    }
    Some(count)
}

/// Les cartes de `p` à partir de `index` peuvent-elles être prises ?
pub fn can_pick<C: Code>(piles: &[Vec<C>], p: usize, index: usize) -> bool {
    p < STORES && index < piles[p].len() && sequences(&piles[p][index..]).is_some()
}

/// `Simon::checkAdd`.
pub fn can_drop<C: Code>(piles: &[Vec<C>], from: usize, index: usize, to: usize) -> bool {
    let moving = &piles[from][index..];
    let Some(first) = moving.first().map(|c| c.code()) else { return false };
    if to >= STORES {
        return piles[to].is_empty()
            && moving.len() == 13
            && rank(first) == 13
            && moving.windows(2).all(|w| suit(w[0].code()) == suit(w[1].code()) && rank(w[1].code()) + 1 == rank(w[0].code()));
    }
    if let Some(top) = piles[to].last()
        && rank(top.code()) != rank(first) + 1
    {
        return false;
    }
    let Some(seqs) = sequences(moving) else { return false };
    let empty = (0..STORES).filter(|&i| i != to && piles[i].is_empty()).count();
    seqs <= 1 << empty
}

pub fn is_legal<C: Code>(piles: &[Vec<C>], from: usize, index: usize, to: usize) -> bool {
    from != to && from < PILES && to < PILES && can_pick(piles, from, index) && can_drop(piles, from, index, to)
}

/// La première suite complète Roi → As au bas d'une colonne, vers la première
/// fondation vide : le dépôt automatique de KPatience (priorité 127).
pub fn autodrop<C: Code>(piles: &[Vec<C>]) -> Option<Move> {
    let to = (FOUNDATION..PILES).find(|&f| piles[f].is_empty())?;
    (0..STORES).find_map(|from| {
        let n = piles[from].len();
        (n >= 13 && can_drop(piles, from, n - 13, to)).then(|| Move::Cards { from, index: n - 13, to })
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `simon` : distingue une partie de Simon le simple des autres jeux dans
/// la partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub simon: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Simon::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        let mut take = || Card { face_up: true, ..list.pop().expect("paquet complet") };
        for count in (3..=9).rev() {
            for pile in piles.iter_mut().take(count) {
                pile.push(take());
            }
        }
        for pile in piles.iter_mut().take(STORES) {
            pile.push(take());
        }
        Game { simon: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..].iter().all(|p| p.len() == 13)
    }

    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        can_pick(&self.piles, p, index)
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => false,
            Move::Cards { from, index, to } => is_legal(&self.piles, from, index, to),
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

    pub fn autodrop(&self) -> Option<Move> {
        autodrop(&self.piles)
    }

    /// Double-clic : une suite complète va sur une fondation ; sinon la suite
    /// va sur la meilleure colonne (même couleur, puis une autre carte, puis
    /// une colonne vide — `Simon::checkPrefering`).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        if let Some(to) = (FOUNDATION..PILES).find(|&f| can_drop(&self.piles, from, index, f)) {
            return Some(Move::Cards { from, index, to });
        }
        let head = self.piles[from][index];
        let mut best: Option<(i32, usize)> = None;
        for to in 0..STORES {
            if to == from || !can_drop(&self.piles, from, index, to) {
                continue;
            }
            let score = match self.piles[to].last() {
                Some(top) if top.suit == head.suit => 3,
                Some(_) => 2,
                None if index > 0 => 1,
                None => continue,
            };
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, to));
            }
        }
        best.map(|(_, to)| Move::Cards { from, index, to })
    }

    /// Les coups utiles, pour les conseils quand l'outil de résolution ne
    /// connaît pas de chemin gagnant : ceux qui posent une suite sur une
    /// carte (sans la retirer d'une carte qui lui convenait déjà), ou qui
    /// libèrent une carte en la posant sur une colonne vide.
    pub fn sensible_moves(&self) -> Vec<Move> {
        let mut result = Vec::new();
        if let Some(m) = self.autodrop() {
            result.push(m);
        }
        for from in 0..STORES {
            let pile = &self.piles[from];
            for index in 0..pile.len() {
                if !self.can_pick(from, index) {
                    continue;
                }
                let placed = index > 0 && pile[index - 1].rank == pile[index].rank + 1 && pile[index - 1].suit == pile[index].suit;
                for to in 0..STORES {
                    if to == from || !can_drop(&self.piles, from, index, to) {
                        continue;
                    }
                    let to_empty = self.piles[to].is_empty();
                    if !placed && (!to_empty || index > 0) {
                        result.push(Move::Cards { from, index, to });
                    }
                }
            }
        }
        result
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
        let counts: Vec<usize> = g.piles[..STORES].iter().map(Vec::len).collect();
        assert_eq!(counts, vec![8, 8, 8, 7, 6, 5, 4, 3, 2, 1]);
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
        // 8♠ 7♥ 6♥ 5♣ : 3 tronçons ; sur 9♦, il faut 2 colonnes vides.
        g.piles[0] = vec![card(8, Suit::Spades), card(7, Suit::Hearts), card(6, Suit::Hearts), card(5, Suit::Clubs)];
        g.piles[1] = vec![card(9, Suit::Diamonds)];
        for p in g.piles[2..STORES].iter_mut() {
            p.push(card(13, Suit::Clubs));
        }
        assert_eq!(sequences(&g.piles[0]), Some(3));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 0, to: 1 }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 2, to: 1 }));
        g.piles[2].clear();
        g.piles[3].clear();
        assert!(g.is_legal(Move::Cards { from: 0, index: 0, to: 1 }));
        // Une suite non descendante ne se prend pas.
        g.piles[4] = vec![card(4, Suit::Clubs), card(2, Suit::Clubs)];
        assert!(!g.can_pick(4, 0));
    }

    #[test]
    fn complete_suit_goes_up() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[3] = std::iter::once(card(5, Suit::Clubs)).chain((1..=13).rev().map(|r| card(r, Suit::Hearts))).collect();
        assert_eq!(g.autodrop(), Some(Move::Cards { from: 3, index: 1, to: FOUNDATION }));
        assert!(g.play(g.autodrop().unwrap()));
        assert_eq!(g.piles[3].len(), 1);
        assert_eq!(g.auto_move(3, 0), None);
    }
}
