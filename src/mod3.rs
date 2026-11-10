//! Règles de Mod3 (comme dans KPatience, `mod3.cpp`).
//!
//! - 104 cartes (2 jeux) ; 4 rangées de 8 piles, une carte visible sur
//!   chacune, le reste au talon ;
//! - les 3 premières rangées sont des fondations : la 1re se monte à partir
//!   d'un 2 (2, 5, 8, Valet), la 2e d'un 3 (3, 6, 9, Dame), la 3e d'un 4 (4,
//!   7, 10, Roi), de 3 en 3 dans la même couleur ; une pile mal commencée ne
//!   reçoit rien ;
//! - la 4e rangée est la réserve : une pile vide y reçoit n'importe quelle
//!   carte ; vidée, elle est aussitôt remplie par le talon ;
//! - les As vont sur leur pile à part ;
//! - seule la carte du dessus d'une pile (fondations et réserve) se déplace ;
//! - « Distribuer » pose une carte du talon sur chaque pile de la réserve ;
//! - la partie est gagnée quand toutes les cartes sont sur les fondations et
//!   la pile des As.
//!
//! Piles : rangée r, colonne c → r × 8 + c (0 à 31) ; As 32 ; talon 33.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const COLUMNS: usize = 8;
/// Les piles des 3 rangées de fondations, puis celles de la réserve.
pub const ROWS: usize = 4;
pub const RESERVE: usize = 24;
pub const ACES: usize = 32;
pub const TALON: usize = 33;
pub const PILES: usize = 34;

/// Rang de départ de la rangée `row` (0 à 2) : 2, 3 ou 4.
pub fn base(row: usize) -> u8 {
    row as u8 + 2
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `mod3` : distingue une partie de Mod3 des autres jeux dans la partie
/// enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub mod3: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Mod3::restart`).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(2, &Suit::ALL);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        // Tout va au talon dans l'ordre de la liste : la dernière carte est
        // sur le dessus.
        piles[TALON] = cards::shuffled(&deck, deal_number).into_iter().map(|c| Card { face_up: false, ..c }).collect();
        for p in 0..ROWS * COLUMNS {
            let c = piles[TALON].pop().expect("talon");
            piles[p].push(Card { face_up: true, ..c });
        }
        Game { mod3: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[RESERVE..ACES].iter().all(Vec::is_empty) && self.piles[TALON].is_empty()
    }

    pub fn can_deal(&self) -> bool {
        !self.piles[TALON].is_empty()
    }

    /// `Mod3::checkRemove` : la carte du dessus d'une fondation ou de la
    /// réserve.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < ACES && index + 1 == self.piles[p].len()
    }

    /// `Mod3::checkAdd`.
    pub fn can_drop(&self, from: usize, index: usize, to: usize) -> bool {
        let Some(card) = self.piles[from].get(index) else { return false };
        let pile = &self.piles[to];
        match to {
            ACES => card.rank == 1,
            TALON => false,
            RESERVE.. => pile.is_empty(),
            _ => {
                let b = base(to / COLUMNS);
                match (pile.first(), pile.last()) {
                    (Some(first), Some(top)) => first.rank == b && card.suit == top.suit && card.rank == top.rank + 3,
                    _ => card.rank == b,
                }
            }
        }
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_deal(),
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
        match m {
            Move::Deal => {
                // `Mod3::newCards` : une carte sur chaque pile de la réserve.
                for p in RESERVE..ACES {
                    let Some(c) = self.piles[TALON].pop() else { break };
                    self.piles[p].push(Card { face_up: true, ..c });
                }
            }
            Move::Cards { from, index, to } => {
                let card = self.piles[from].remove(index);
                self.piles[to].push(card);
                // `Mod3::cardsMoved` : une pile de la réserve vidée reçoit
                // aussitôt la carte du dessus du talon.
                if (RESERVE..ACES).contains(&from)
                    && self.piles[from].is_empty()
                    && let Some(c) = self.piles[TALON].pop()
                {
                    self.piles[from].push(Card { face_up: true, ..c });
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

    /// Le dépôt automatique de KPatience (`DealerScene::drop`) : seuls les
    /// coups de priorité > 120 de l'outil de résolution, c'est-à-dire les As
    /// vers leur pile.
    pub fn autodrop(&self) -> Option<Move> {
        (0..ACES).find_map(|from| {
            let pile = &self.piles[from];
            pile.last().filter(|c| c.rank == 1).map(|_| Move::Cards { from, index: pile.len() - 1, to: ACES })
        })
    }

    /// Double-clic : un As va sur sa pile ; sinon la carte va sur une
    /// fondation qui la prend (une pile commencée plutôt qu'une vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let mv = |to| Move::Cards { from, index, to };
        if self.can_drop(from, index, ACES) {
            return Some(mv(ACES));
        }
        let legal = |to: &usize| *to != from && self.can_drop(from, index, *to);
        (0..RESERVE)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..RESERVE).filter(legal).find(|&to| to / COLUMNS != from / COLUMNS || self.piles[from].len() > 1))
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
        assert!(g.piles[..ACES].iter().all(|p| p.len() == 1 && p[0].face_up));
        assert_eq!(g.piles[TALON].len(), 104 - 32);
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..104).collect::<Vec<u8>>());
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[TALON] = vec![card(9, Suit::Clubs)];
        g.piles[0] = vec![card(2, Suit::Hearts)];
        g.piles[1] = vec![card(7, Suit::Hearts)];
        g.piles[RESERVE] = vec![card(5, Suit::Hearts)];
        g.piles[RESERVE + 1] = vec![card(5, Suit::Spades)];
        g.piles[RESERVE + 2] = vec![card(1, Suit::Spades)];
        // De 3 en 3 dans la même couleur, sur une pile bien commencée.
        assert!(!g.is_legal(Move::Cards { from: RESERVE + 1, index: 0, to: 0 }));
        assert!(!g.is_legal(Move::Cards { from: RESERVE, index: 0, to: 1 }));
        assert!(g.play(Move::Cards { from: RESERVE, index: 0, to: 0 }));
        // La pile de réserve vidée reprend la carte du talon.
        assert_eq!(g.piles[RESERVE].len(), 1);
        assert!(g.piles[TALON].is_empty());
        // Une pile vide de la 2e rangée ne prend qu'un 3.
        g.piles[COLUMNS].clear();
        assert!(!g.is_legal(Move::Cards { from: RESERVE + 1, index: 0, to: COLUMNS }));
        // Les As vont d'office sur leur pile ; on ne les reprend pas.
        assert_eq!(g.autodrop(), Some(Move::Cards { from: RESERVE + 2, index: 0, to: ACES }));
        assert!(g.play(Move::Cards { from: RESERVE + 2, index: 0, to: ACES }));
        assert!(!g.can_pick(ACES, 0));
        // Une pile vide de la réserve prend n'importe quelle carte.
        assert!(g.is_legal(Move::Cards { from: 1, index: 0, to: RESERVE + 2 }));
    }

    #[test]
    fn deal_puts_one_card_on_each_reserve_pile() {
        let mut g = Game::new(3);
        assert!(g.play(Move::Deal));
        assert!(g.piles[RESERVE..ACES].iter().all(|p| p.len() == 2));
        assert_eq!(g.piles[TALON].len(), 104 - 40);
        while g.can_deal() {
            assert!(g.play(Move::Deal));
        }
        assert!(!g.play(Move::Deal));
    }
}
