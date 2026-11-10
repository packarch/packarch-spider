//! Règles du Klondike (comme dans KPatience, `klondike.cpp`).
//!
//! - 52 cartes ; 7 colonnes de 1 à 7 cartes, seule la dernière face visible ;
//!   le reste forme le talon ;
//! - un clic sur le talon retourne 1 ou 3 cartes sur la pioche (selon la
//!   variante) ; talon vide, la pioche y est retournée (sans limite) ;
//! - dans les colonnes, on descend en alternant rouge et noir ; une colonne
//!   vide ne reçoit qu'un Roi ; la carte découverte se retourne ;
//! - 4 fondations, de l'As au Roi par couleur ; en pioche 1, on peut reprendre
//!   la carte du dessus d'une fondation.
//!
//! Piles : colonnes 0 à 6, pioche 7, talon 8, fondations 9 à 12 (le même
//! ordre que l'outil de résolution de KPatience).

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const TABLEAU: usize = 7;
pub const WASTE: usize = 7;
pub const STOCK: usize = 8;
pub const FOUNDATION: usize = 9;
pub const PILES: usize = 13;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PileKind {
    Tableau,
    Waste,
    Stock,
    Foundation,
}

pub fn kind(p: usize) -> PileKind {
    match p {
        0..TABLEAU => PileKind::Tableau,
        WASTE => PileKind::Waste,
        STOCK => PileKind::Stock,
        _ => PileKind::Foundation,
    }
}

/// `card` peut-elle être posée sur `below` dans une colonne ?
fn builds_on(card: &Card, below: &Card) -> bool {
    below.face_up && card.rank + 1 == below.rank && card.suit.is_red() != below.suit.is_red()
}

fn is_alternate_descending(cards: &[Card]) -> bool {
    cards.iter().all(|c| c.face_up) && cards.windows(2).all(|w| builds_on(&w[1], &w[0]))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    /// Cartes retournées à chaque pioche : 1 ou 3.
    pub draw: u8,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Klondike::restart`).
    pub fn new(draw: u8, deal_number: u32) -> Game {
        let draw = if draw == 3 { 3 } else { 1 };
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for round in 0..TABLEAU {
            for (i, pile) in piles.iter_mut().enumerate().take(TABLEAU).skip(round) {
                let mut c = list.pop().expect("paquet complet");
                c.face_up = i == round;
                pile.push(c);
            }
        }
        // Le reste va au talon dans l'ordre de la liste : la dernière carte
        // est sur le dessus.
        piles[STOCK] = list.into_iter().map(|c| Card { face_up: false, ..c }).collect();
        Game { draw, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..].iter().map(Vec::len).sum::<usize>() == 52
    }

    /// Peut-on piocher (ou retourner la pioche) ? (`Klondike::newCards`)
    pub fn can_draw(&self) -> bool {
        !self.piles[STOCK].is_empty() || self.piles[WASTE].len() > 1
    }

    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        let Some(pile) = self.piles.get(p) else { return false };
        if index >= pile.len() {
            return false;
        }
        match kind(p) {
            PileKind::Tableau => is_alternate_descending(&pile[index..]),
            PileKind::Waste => index + 1 == pile.len(),
            PileKind::Foundation => self.draw == 1 && index + 1 == pile.len(),
            PileKind::Stock => false,
        }
    }

    pub fn can_drop(&self, from: usize, index: usize, to: usize) -> bool {
        let moving = &self.piles[from][index..];
        let Some(first) = moving.first() else { return false };
        let top = self.piles[to].last();
        match kind(to) {
            PileKind::Tableau => match top {
                None => first.rank == 13,
                Some(t) => builds_on(first, t),
            },
            PileKind::Foundation => {
                moving.len() == 1
                    && match top {
                        None => first.rank == 1,
                        Some(t) => t.suit == first.suit && t.rank + 1 == first.rank,
                    }
            }
            PileKind::Waste | PileKind::Stock => false,
        }
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_draw(),
            Move::Cards { from, index, to } => {
                from != to && from < PILES && to < PILES && self.can_pick(from, index) && self.can_drop(from, index, to)
            }
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        self.undo.push(Snapshot { piles: self.piles.clone(), moves: self.moves });
        self.redo.clear();
        let piles = &mut self.piles;
        match m {
            Move::Deal => {
                if piles[STOCK].is_empty() {
                    // La pioche est retournée sur le talon.
                    while let Some(c) = piles[WASTE].pop() {
                        piles[STOCK].push(Card { face_up: false, ..c });
                    }
                } else {
                    // Les cartes du dessus du talon, retournées en paquet :
                    // la plus profonde finit sur le dessus de la pioche.
                    let n = (self.draw as usize).min(piles[STOCK].len());
                    for _ in 0..n {
                        let c = piles[STOCK].pop().expect("talon non vide");
                        piles[WASTE].push(Card { face_up: true, ..c });
                    }
                }
            }
            Move::Cards { from, index, to } => {
                let moving = piles[from].split_off(index);
                piles[to].extend(moving);
                if kind(from) == PileKind::Tableau {
                    if let Some(top) = piles[from].last_mut() {
                        top.face_up = true;
                    }
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
    /// vide pour un As (`DealerScene::tryAutomaticMove`).
    pub fn foundation_for(&self, card: &Card) -> Option<usize> {
        (FOUNDATION..PILES).find(|&f| match self.piles[f].last() {
            None => card.rank == 1,
            Some(t) => t.suit == card.suit && t.rank + 1 == card.rank,
        })
    }

    /// Hauteur de la fondation de la couleur `suit`.
    fn out(&self, suit: Suit) -> u8 {
        self.piles[FOUNDATION..].iter().filter_map(|p| p.last()).find(|t| t.suit == suit).map_or(0, |t| t.rank)
    }

    /// Sortie sans risque (`KlondikeSolver::good_automove`, règle de Raymond).
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

    /// Le dépôt automatique de KPatience : la première sortie sans risque
    /// (colonnes, puis pioche ; en pioche 3, la pioche seulement s'il y reste
    /// moins de 3 cartes, pour ne pas décaler les paquets).
    pub fn autodrop(&self) -> Option<Move> {
        for from in 0..=WASTE {
            let pile = &self.piles[from];
            let Some(card) = pile.last() else { continue };
            if !card.face_up {
                continue;
            }
            let Some(to) = self.foundation_for(card) else { continue };
            if self.good_automove(card) && (from != WASTE || self.draw == 1 || pile.len() < 3) {
                return Some(Move::Cards { from, index: pile.len() - 1, to });
            }
        }
        None
    }

    /// `Klondike::drop` : quand le dépôt automatique vide la pioche, on
    /// pioche aussitôt.
    pub fn draw_after_drop(&self, m: Move) -> bool {
        matches!(m, Move::Cards { from: WASTE, .. }) && self.piles[WASTE].is_empty() && !self.piles[STOCK].is_empty()
    }

    /// Double-clic : la carte du dessus va sur une fondation ; sinon la carte
    /// (ou la suite) va sur une colonne (une carte plutôt qu'une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let pile = &self.piles[from];
        let mv = |to| Move::Cards { from, index, to };
        if index + 1 == pile.len() && kind(from) != PileKind::Foundation {
            if let Some(to) = self.foundation_for(&pile[index]) {
                return Some(mv(to));
            }
        }
        let legal = |to: &usize| *to != from && self.can_drop(from, index, *to);
        (0..TABLEAU)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..TABLEAU).filter(legal).find(|_| !(index == 0 && kind(from) == PileKind::Tableau)))
            .map(mv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_deal() {
        let g = Game::new(1, 1);
        let counts: Vec<usize> = g.piles[..TABLEAU].iter().map(Vec::len).collect();
        assert_eq!(counts, vec![1, 2, 3, 4, 5, 6, 7]);
        for p in &g.piles[..TABLEAU] {
            assert!(p.last().unwrap().face_up);
            assert!(p[..p.len() - 1].iter().all(|c| !c.face_up));
        }
        assert_eq!(g.piles[STOCK].len(), 24);
        assert!(g.piles[STOCK].iter().all(|c| !c.face_up));
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn draw_three_and_redeal() {
        let mut g = Game::new(3, 7);
        let stock: Vec<Card> = g.piles[STOCK].clone();
        assert!(g.play(Move::Deal));
        // Le paquet retourné : l'ancienne 3e carte du dessus est visible.
        assert_eq!(g.piles[WASTE].len(), 3);
        assert_eq!(g.piles[WASTE].last().unwrap().id, stock[stock.len() - 3].id);
        assert_eq!(g.piles[WASTE][0].id, stock[stock.len() - 1].id);
        for _ in 0..7 {
            assert!(g.play(Move::Deal));
        }
        assert!(g.piles[STOCK].is_empty());
        // Retourner la pioche rend le talon dans son ordre de départ.
        assert!(g.play(Move::Deal));
        let ids: Vec<u8> = g.piles[STOCK].iter().map(|c| c.id).collect();
        assert_eq!(ids, stock.iter().map(|c| c.id).collect::<Vec<u8>>());
        assert!(g.piles[STOCK].iter().all(|c| !c.face_up));
        assert!(g.undo());
        assert!(g.piles[STOCK].is_empty());
    }

    #[test]
    fn tableau_rules() {
        let mut g = Game::new(1, 1);
        let c = |rank, suit, face_up| Card { id: 0, rank, suit, face_up };
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![c(5, Suit::Clubs, false), c(9, Suit::Hearts, true)];
        g.piles[1] = vec![c(10, Suit::Spades, true)];
        g.piles[2] = vec![c(13, Suit::Diamonds, true), c(12, Suit::Clubs, true)];
        assert!(g.play(Move::Cards { from: 0, index: 1, to: 1 }));
        // La carte découverte se retourne.
        assert!(g.piles[0][0].face_up);
        // Une colonne vide ne reçoit qu'un Roi.
        g.piles[3].clear();
        assert!(!g.is_legal(Move::Cards { from: 1, index: 1, to: 3 }));
        assert!(g.is_legal(Move::Cards { from: 2, index: 0, to: 3 }));
        // Pas de suite non alternée.
        assert!(!g.can_pick(1, 0) || is_alternate_descending(&g.piles[1]));
    }
}
