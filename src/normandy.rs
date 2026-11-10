//! Règles de Normandie (Normandy), comme dans KPatience (`normandy.cpp`).
//!
//! - 104 cartes (2 jeux) : 8 colonnes d'une carte cachée et de deux cartes
//!   visibles ; les 80 autres au talon ;
//! - 8 fondations, de l'As au Roi dans la même couleur ; la carte du dessus
//!   d'une fondation peut redescendre sur une colonne ;
//! - sur une colonne, on descend en alternant rouge et noir ; toute suite
//!   ainsi rangée se déplace d'un bloc, sans limite ; une colonne vide
//!   reçoit n'importe quelle suite ;
//! - « Distribuer » pose une carte du talon sur chaque colonne ; pas de
//!   redistribution.
//!
//! Piles : colonnes 0 à 7, fondations 8 à 15, talon 16.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

pub const STORES: usize = 8;
pub const FOUNDATION: usize = 8;
pub const TALON: usize = 16;
pub const PILES: usize = 17;

fn red(c: &Card) -> bool {
    c.suit.is_red()
}

/// `card` peut-elle aller sur `below` dans une colonne ?
pub fn builds_on(card: &Card, below: &Card) -> bool {
    card.rank + 1 == below.rank && red(card) != red(below)
}

/// `isAlternateColorDescending`, cartes visibles.
pub fn is_run(cards: &[Card]) -> bool {
    cards.iter().all(|c| c.face_up) && cards.windows(2).all(|w| builds_on(&w[1], &w[0]))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `normandy` : distingue une partie de Normandie des autres jeux dans la
/// partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub normandy: bool,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

/// Hauteurs des fondations de la couleur `s` (0 si pas commencée), la plus
/// haute d'abord : toujours 2 valeurs.
fn heights(piles: &[Vec<Card>], s: Suit) -> [u8; 2] {
    let mut h = [0u8; 2];
    let mut r: Vec<u8> = piles[FOUNDATION..TALON].iter().filter_map(|p| p.last()).filter(|t| t.suit == s).map(|t| t.rank).collect();
    r.sort_unstable_by(|a, b| b.cmp(a));
    for (k, x) in r.into_iter().take(2).enumerate() {
        h[k] = x;
    }
    h
}

/// `NormandySolver::good_automove` (règle de Raymond, 4 fondations par
/// teinte) : une carte de rang `r` et de couleur `s` peut-elle sortir sans
/// risque ?
pub fn good_automove(piles: &[Vec<Card>], s: Suit, r: u8) -> bool {
    if r <= 2 {
        return true;
    }
    let tint = |red: bool| -> Vec<u8> { Suit::ALL.into_iter().filter(|t| t.is_red() == red).flat_map(|t| heights(piles, t)).collect() };
    let opposite = tint(!s.is_red());
    if opposite.iter().all(|&h| h + 1 >= r) {
        return true;
    }
    opposite.iter().all(|&h| h + 2 >= r) && tint(s.is_red()).iter().all(|&h| h + 3 >= r)
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Normandy::restart`) :
    /// 3 rangées prises à la fin de la liste (la première cachée), le reste
    /// au talon dans l'ordre de la liste (la dernière carte sur le dessus).
    pub fn new(deal_number: u32) -> Game {
        let deck = cards::deck(2, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for row in 0..3 {
            for pile in piles.iter_mut().take(STORES) {
                pile.push(Card { face_up: row > 0, ..list.pop().expect("paquet complet") });
            }
        }
        piles[TALON] = list.into_iter().map(|c| Card { face_up: false, ..c }).collect();
        Game { normandy: true, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..TALON].iter().all(|p| p.len() == 13)
    }

    pub fn can_deal(&self) -> bool {
        !self.piles[TALON].is_empty()
    }

    /// `Normandy::checkRemove`.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        let pile = &self.piles[p];
        match p {
            0..STORES => index < pile.len() && is_run(&pile[index..]),
            FOUNDATION..TALON => index + 1 == pile.len(),
            _ => false,
        }
    }

    fn fits_foundation(&self, f: usize, card: &Card) -> bool {
        match self.piles[f].last() {
            None => card.rank == 1,
            Some(t) => t.suit == card.suit && t.rank + 1 == card.rank,
        }
    }

    pub fn foundation_for(&self, card: &Card) -> Option<usize> {
        (FOUNDATION..TALON).find(|&f| self.fits_foundation(f, card))
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => self.can_deal(),
            Move::Cards { from, index, to } => {
                if from == to || to >= TALON || !self.can_pick(from, index) {
                    return false;
                }
                let card = &self.piles[from][index];
                if to >= FOUNDATION {
                    index + 1 == self.piles[from].len() && self.fits_foundation(to, card)
                } else {
                    self.piles[to].last().is_none_or(|t| builds_on(card, t))
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
            Move::Deal => {
                for i in 0..STORES {
                    let Some(c) = self.piles[TALON].pop() else { break };
                    self.piles[i].push(Card { face_up: true, ..c });
                }
            }
            Move::Cards { from, index, to } => {
                let moving = self.piles[from].split_off(index);
                self.piles[to].extend(moving);
                if from < STORES
                    && let Some(top) = self.piles[from].last_mut()
                {
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

    /// Le dépôt automatique de KPatience (`DealerScene::drop`, sorties de
    /// priorité 127 de `NormandySolver`) : la première sortie sans risque
    /// depuis le dessus d'une colonne.
    pub fn autodrop(&self) -> Option<Move> {
        (0..STORES).find_map(|from| {
            let card = self.piles[from].last()?;
            let to = self.foundation_for(card)?;
            good_automove(&self.piles, card.suit, card.rank).then(|| Move::Cards { from, index: self.piles[from].len() - 1, to })
        })
    }

    /// Double-clic : la carte du dessus va sur une fondation ; sinon la suite
    /// va sur une autre colonne (une carte plutôt qu'une colonne vide, et pas
    /// toute une colonne vers une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let mv = |to| Move::Cards { from, index, to };
        if from < STORES
            && index + 1 == self.piles[from].len()
            && let Some(to) = self.foundation_for(&self.piles[from][index])
        {
            return Some(mv(to));
        }
        let legal = |to: &usize| self.is_legal(mv(*to));
        (0..STORES)
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..STORES).filter(legal).find(|_| from >= STORES || index > 0))
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
        let counts: Vec<usize> = g.piles.iter().map(Vec::len).collect();
        assert_eq!(counts, [vec![3; 8], vec![0; 8], vec![80]].concat());
        for col in &g.piles[..STORES] {
            assert!(!col[0].face_up && col[1].face_up && col[2].face_up);
        }
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..104).collect::<Vec<u8>>());
        let list = cards::shuffled(&cards::deck(2, &Suit::ALL), 1);
        assert_eq!(g.piles[0][0].id, list[103].id);
        assert_eq!(g.piles[7][2].id, list[80].id);
        assert_eq!(g.piles[TALON][0].id, list[0].id);
    }

    #[test]
    fn rules() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] =
            vec![card(2, Suit::Clubs, false), card(9, Suit::Hearts, true), card(8, Suit::Spades, true), card(7, Suit::Diamonds, true)];
        g.piles[1] = vec![card(10, Suit::Clubs, true)];
        g.piles[2] = vec![card(8, Suit::Clubs, true)];
        // Une suite entière, sans limite ; jamais une carte cachée.
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(!g.can_pick(0, 0));
        assert!(g.is_legal(Move::Cards { from: 0, index: 3, to: 2 }));
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 5 }), "colonne vide : n'importe quelle suite");
        assert!(g.play(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(g.piles[0][0].face_up, "la carte découverte se retourne");
        // Une carte redescend d'une fondation.
        g.piles[FOUNDATION] = vec![card(1, Suit::Hearts, true), card(2, Suit::Hearts, true)];
        g.piles[3] = vec![card(3, Suit::Spades, true)];
        assert!(g.is_legal(Move::Cards { from: FOUNDATION, index: 1, to: 3 }));
        assert!(!g.is_legal(Move::Cards { from: FOUNDATION, index: 0, to: 3 }));
    }

    #[test]
    fn deal_rows() {
        let mut g = Game::new(3);
        for k in 1..=10 {
            assert!(g.play(Move::Deal));
            assert_eq!(g.piles[TALON].len(), 80 - 8 * k);
        }
        assert!(!g.can_deal());
        assert!(g.piles[..STORES].iter().all(|p| p.len() == 13 && p[1..].iter().all(|c| c.face_up)));
    }

    #[test]
    fn raymond_rule() {
        let mut g = Game::new(1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        // 5 de cœur : les noirs (4 fondations) doivent être à 4, sinon à 3
        // avec les rouges à 2.
        assert!(!good_automove(&g.piles, Suit::Hearts, 5));
        for (i, s) in [Suit::Clubs, Suit::Clubs, Suit::Spades, Suit::Spades].into_iter().enumerate() {
            g.piles[FOUNDATION + i] = (1..=4).map(|r| card(r, s, true)).collect();
        }
        assert!(good_automove(&g.piles, Suit::Hearts, 5));
        g.piles[FOUNDATION].pop();
        assert!(!good_automove(&g.piles, Suit::Hearts, 5));
        for (i, s) in [Suit::Hearts, Suit::Hearts, Suit::Diamonds, Suit::Diamonds].into_iter().enumerate() {
            g.piles[FOUNDATION + 4 + i] = (1..=2).map(|r| card(r, s, true)).collect();
        }
        assert!(good_automove(&g.piles, Suit::Hearts, 5));
    }
}
