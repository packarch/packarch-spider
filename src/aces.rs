//! Règles des As (Aces Up), comme dans KPatience (`idiot.cpp`).
//!
//! - 52 cartes : tout le paquet au talon, puis une carte visible sur chacune
//!   des 4 colonnes ;
//! - la carte du dessus d'une colonne est écartée si une autre colonne
//!   montre une carte plus haute de la même couleur (l'As est la plus
//!   haute) ;
//! - une colonne vide reçoit la carte du dessus de n'importe quelle autre ;
//! - « Distribuer » pose une carte du talon sur chaque colonne ;
//! - la partie est gagnée quand le talon est vide et qu'il ne reste que les
//!   4 As, un par colonne.
//!
//! Piles : colonnes 0 à 3, cartes écartées 4, talon 5.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 4;
pub const AWAY: usize = 4;
pub const TALON: usize = 5;
pub const PILES: usize = 6;

/// `higher` de `IdiotSolver` : `top` (même couleur) permet-elle d'écarter
/// `card` ? L'As est plus haut que tout.
pub fn beats(top: Card, card: Card) -> bool {
    top.suit == card.suit && top.rank != card.rank && (top.rank == 1 || (card.rank != 1 && top.rank > card.rank))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `aces` : distingue une partie des As des autres jeux dans la partie
/// enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub aces: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Idiot::restart`) :
    /// le paquet au talon dans l'ordre de la liste (la dernière carte sur le
    /// dessus), puis une rangée.
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        piles[TALON] = list.into_iter().map(|c| Card { face_up: false, ..c }).collect();
        let mut g = Game { aces: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() };
        g.deal_row();
        g
    }

    fn deal_row(&mut self) {
        for i in 0..STORES {
            let c = self.piles[TALON].pop().expect("talon");
            self.piles[i].push(Card { face_up: true, ..c });
        }
    }

    /// `Idiot::isGameWon`.
    pub fn is_won(&self) -> bool {
        self.piles[TALON].is_empty() && self.piles[..STORES].iter().all(|p| p.len() == 1 && p[0].rank == 1)
    }

    /// Reste-t-il une rangée à distribuer ? (`Idiot::newCards`)
    pub fn can_deal(&self) -> bool {
        self.piles[TALON].len() >= STORES
    }

    /// `Idiot::canMoveAway` : la carte du dessus de la colonne `p` peut-elle
    /// être écartée ?
    pub fn can_move_away(&self, p: usize) -> bool {
        let Some(&card) = self.piles[p].last() else { return false };
        (0..STORES).any(|i| i != p && self.piles[i].last().is_some_and(|&t| beats(t, card)))
    }

    fn has_empty_column(&self) -> bool {
        self.piles[..STORES].iter().any(Vec::is_empty)
    }

    /// `Idiot::checkRemove` : la carte du dessus d'une colonne, si elle peut
    /// être écartée ou s'il y a une colonne vide.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES && index + 1 == self.piles[p].len() && (self.can_move_away(p) || self.has_empty_column())
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_deal(),
            Move::Cards { from, index, to } => {
                from < STORES
                    && index + 1 == self.piles[from].len()
                    && match to {
                        AWAY => self.can_move_away(from),
                        t if t < STORES => t != from && self.piles[t].is_empty(),
                        _ => false,
                    }
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
            Move::Deal => self.deal_row(),
            Move::Cards { from, to, .. } => {
                let card = self.piles[from].pop().expect("colonne");
                self.piles[to].push(card);
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

    /// Clic ou double-clic (`Idiot::tryAutomaticMove`) : la carte du dessus
    /// est écartée si elle peut l'être, sinon elle va sur la première
    /// colonne vide.
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if from >= STORES || index + 1 != self.piles[from].len() {
            return None;
        }
        if self.can_move_away(from) {
            return Some(Move::Cards { from, index, to: AWAY });
        }
        (0..STORES).find(|&t| t != from && self.piles[t].is_empty()).map(|to| Move::Cards { from, index, to })
    }

    /// Les coups possibles, pour les conseils quand l'outil de résolution ne
    /// connaît pas de chemin gagnant : les cartes à écarter, puis celles qui
    /// peuvent aller sur une colonne vide (depuis une colonne d'au moins
    /// 2 cartes : déplacer une carte seule ne libère rien).
    pub fn sensible_moves(&self) -> Vec<Move> {
        let top = |p: usize| self.piles[p].len().checked_sub(1);
        let mut moves: Vec<Move> = (0..STORES)
            .filter(|&p| self.can_move_away(p))
            .filter_map(|p| top(p).map(|index| Move::Cards { from: p, index, to: AWAY }))
            .collect();
        if moves.is_empty()
            && let Some(to) = (0..STORES).find(|&t| self.piles[t].is_empty())
        {
            let from_tall = (0..STORES).filter(|&p| self.piles[p].len() > 1);
            moves.extend(from_tall.map(|p| Move::Cards { from: p, index: self.piles[p].len() - 1, to }));
        }
        moves
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
        assert_eq!(counts, vec![1, 1, 1, 1, 0, 48]);
        assert!(g.piles[..STORES].iter().flatten().all(|c| c.face_up));
        assert!(g.piles[TALON].iter().all(|c| !c.face_up));
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
        // Comme KPatience : la colonne 0 reçoit la dernière carte de la liste.
        let list = cards::shuffled(&cards::deck(1, &Suit::ALL), 1);
        assert_eq!(g.piles[0][0].id, list[51].id);
        assert_eq!(g.piles[3][0].id, list[48].id);
    }

    #[test]
    fn rules() {
        assert!(beats(card(9, Suit::Hearts), card(4, Suit::Hearts)));
        assert!(beats(card(1, Suit::Hearts), card(13, Suit::Hearts)));
        assert!(!beats(card(13, Suit::Hearts), card(1, Suit::Hearts)));
        assert!(!beats(card(9, Suit::Spades), card(4, Suit::Hearts)));
        assert!(!beats(card(4, Suit::Hearts), card(9, Suit::Hearts)));

        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(2, Suit::Clubs), card(5, Suit::Hearts)];
        g.piles[1] = vec![card(11, Suit::Hearts)];
        g.piles[2] = vec![card(7, Suit::Spades), card(3, Suit::Spades)];
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: AWAY }));
        assert!(!g.is_legal(Move::Cards { from: 1, index: 0, to: AWAY }));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 0, to: AWAY }));
        // Colonne 3 vide : toute carte du dessus peut y aller.
        assert!(g.is_legal(Move::Cards { from: 2, index: 1, to: 3 }));
        assert!(!g.is_legal(Move::Cards { from: 2, index: 1, to: 1 }));
        assert!(g.can_pick(1, 0));
        assert_eq!(g.auto_move(0, 1), Some(Move::Cards { from: 0, index: 1, to: AWAY }));
        assert_eq!(g.auto_move(2, 1), Some(Move::Cards { from: 2, index: 1, to: 3 }));
        assert_eq!(g.sensible_moves(), vec![Move::Cards { from: 0, index: 1, to: AWAY }]);
        assert!(!g.can_deal());

        g.piles[3] = vec![card(6, Suit::Diamonds)];
        assert!(!g.can_pick(2, 1));
        assert_eq!(g.auto_move(2, 1), None);
    }

    #[test]
    fn deal_and_undo() {
        let mut g = Game::new(5);
        assert!(g.play(Move::Deal));
        assert_eq!(g.piles[TALON].len(), 44);
        assert!(g.piles[..STORES].iter().all(|p| p.len() == 2 && p.iter().all(|c| c.face_up)));
        assert!(g.undo());
        assert_eq!(g.piles[TALON].len(), 48);
        assert!(g.redo());
        assert_eq!(g.moves, 1);
    }

    #[test]
    fn won_with_four_aces() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        for (i, s) in Suit::ALL.into_iter().enumerate() {
            g.piles[i].push(card(1, s));
        }
        assert!(g.is_won());
        g.piles[0].push(card(2, Suit::Clubs));
        assert!(!g.is_won());
    }
}
