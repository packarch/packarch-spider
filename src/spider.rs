//! Règles du Spider (comme dans KPatience).
//!
//! - 104 cartes (2 paquets) en 1, 2, 3 ou 4 couleurs ;
//! - 10 colonnes : 54 cartes distribuées (6 dans les 4 premières colonnes,
//!   5 dans les autres), seule la dernière de chaque colonne face visible
//!   (ou toutes, avec l'option « cartes visibles ») ;
//! - 5 rangées de 10 cartes en réserve, distribuées une par colonne ;
//! - on déplace une suite de même couleur et de rangs décroissants, sur une
//!   colonne vide ou sur une carte de rang supérieur d'un (couleur libre) ;
//! - une suite complète Roi → As de même couleur part toute seule aux
//!   fondations ; la partie est gagnée avec les 8 suites.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card};

pub const COLUMNS: usize = 10;
pub const ROWS: usize = 5;
pub const RUNS: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    columns: [Vec<Card>; COLUMNS],
    stock: Vec<Vec<Card>>,
    runs: Vec<Vec<Card>>,
    moves: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub suit_count: u8,
    pub deal_number: u32,
    pub stacks_face_up: bool,
    /// Les colonnes, de la carte du fond à celle du dessus.
    pub columns: [Vec<Card>; COLUMNS],
    /// Les rangées restantes de la réserve ; `stock[0]` est la prochaine,
    /// et `stock[r][c]` la carte qui ira sur la colonne `c`.
    pub stock: Vec<Vec<Card>>,
    /// Les suites complètes (Roi → As) déjà sorties.
    pub runs: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Spider::restart`).
    pub fn new(suit_count: u8, deal_number: u32, stacks_face_up: bool) -> Game {
        let deck = cards::spider_deck(suit_count);
        let mut pile = cards::shuffled(&deck, deal_number);
        let mut columns: [Vec<Card>; COLUMNS] = Default::default();

        let mut column = 0;
        // 44 cartes face cachée : 5 dans les 4 premières colonnes, 4 ailleurs…
        for _ in 0..44 {
            let mut c = pile.pop().expect("paquet complet");
            c.face_up = stacks_face_up;
            columns[column].push(c);
            column = (column + 1) % COLUMNS;
        }
        // … puis une carte face visible par colonne.
        for _ in 0..COLUMNS {
            let mut c = pile.pop().expect("paquet complet");
            c.face_up = true;
            columns[column].push(c);
            column = (column + 1) % COLUMNS;
        }
        // Les 50 cartes restantes forment 5 tas de 10 ; on distribue en
        // prenant la carte du dessus de chaque tas, d'où l'ordre inversé.
        let mut stock = Vec::with_capacity(ROWS);
        for _ in 0..ROWS {
            let mut row: Vec<Card> = (0..COLUMNS).map(|_| pile.pop().expect("paquet complet")).collect();
            row.reverse();
            stock.push(row);
        }

        Game { suit_count, deal_number, stacks_face_up, columns, stock, runs: Vec::new(), moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn is_won(&self) -> bool {
        self.runs.len() == RUNS
    }

    /// Les cartes à partir de `index` forment-elles une suite déplaçable ?
    pub fn can_pick(&self, column: usize, index: usize) -> bool {
        let cards = &self.columns[column];
        if index >= cards.len() || !cards[index].face_up {
            return false;
        }
        is_same_suit_descending(&cards[index..])
    }

    /// La carte `card` (tête d'une suite) peut-elle aller sur la colonne `to` ?
    pub fn can_drop(&self, to: usize, card: &Card) -> bool {
        match self.columns[to].last() {
            None => true,
            Some(top) => top.face_up && top.rank == card.rank + 1,
        }
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => !self.stock.is_empty(),
            Move::Cards { from, index, to } => {
                from < COLUMNS && to < COLUMNS && from != to && self.can_pick(from, index) && self.can_drop(to, &self.columns[from][index])
            }
        }
    }

    /// Joue un coup. Renvoie `false` (sans rien changer) s'il est illégal.
    pub fn play(&mut self, m: Move) -> bool {
        if !self.is_legal(m) {
            return false;
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
        match m {
            Move::Deal => {
                let row = self.stock.remove(0);
                for (column, mut card) in row.into_iter().enumerate() {
                    card.face_up = true;
                    self.columns[column].push(card);
                }
                for column in 0..COLUMNS {
                    self.remove_full_run(column);
                }
            }
            Move::Cards { from, index, to } => {
                let moving = self.columns[from].split_off(index);
                self.columns[to].extend(moving);
                self.turn_top(from);
                self.remove_full_run(to);
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

    /// Toutes les cartes du jeu.
    #[cfg(test)]
    pub fn all_cards(&self) -> impl Iterator<Item = &Card> {
        self.columns.iter().flatten().chain(self.stock.iter().flatten()).chain(self.runs.iter().flatten())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { columns: self.columns.clone(), stock: self.stock.clone(), runs: self.runs.clone(), moves: self.moves }
    }

    fn restore(&mut self, s: Snapshot) {
        self.columns = s.columns;
        self.stock = s.stock;
        self.runs = s.runs;
        self.moves = s.moves;
    }

    fn turn_top(&mut self, column: usize) {
        if let Some(top) = self.columns[column].last_mut() {
            top.face_up = true;
        }
    }

    /// Sort la suite Roi → As du dessus de la colonne, s'il y en a une.
    fn remove_full_run(&mut self, column: usize) {
        let cards = &self.columns[column];
        if cards.len() < 13 {
            return;
        }
        let start = cards.len() - 13;
        let run = &cards[start..];
        if run[0].face_up && run[0].rank == 13 && is_same_suit_descending(run) {
            let run = self.columns[column].split_off(start);
            self.runs.push(run);
            self.turn_top(column);
        }
    }

    /// Double-clic : la meilleure colonne pour la suite (même couleur, puis
    /// une autre carte, puis une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let head = self.columns[from][index];
        let mut best: Option<(i32, usize)> = None;
        for to in 0..COLUMNS {
            if to == from || !self.can_drop(to, &head) {
                continue;
            }
            let score = match self.columns[to].last() {
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

    /// Les coups « raisonnables » (comme `bruteForceHints` de KPatience) :
    /// utilisés pour les conseils quand l'outil de résolution ne connaît pas
    /// de chemin gagnant.
    pub fn sensible_moves(&self) -> Vec<Move> {
        let mut result = Vec::new();
        for from in 0..COLUMNS {
            let cards = &self.columns[from];
            for index in 0..cards.len() {
                if !self.can_pick(from, index) {
                    continue;
                }
                let below = if index > 0 { Some(&cards[index - 1]) } else { None };
                // La suite est-elle déjà posée sur une carte qui lui convient ?
                let already_placed = below.is_some_and(|b| b.face_up && b.rank == cards[index].rank + 1);
                for to in 0..COLUMNS {
                    if to == from || !self.can_drop(to, &cards[index]) {
                        continue;
                    }
                    let to_empty = self.columns[to].is_empty();
                    // Déplacer toute une colonne sur une colonne vide ne sert à rien.
                    if index == 0 && to_empty {
                        continue;
                    }
                    if !already_placed || (below.is_none() && !to_empty) {
                        result.push(Move::Cards { from, index, to });
                    }
                }
            }
        }
        result
    }
}

pub fn is_same_suit_descending(cards: &[Card]) -> bool {
    cards.windows(2).all(|w| w[1].face_up && w[1].suit == w[0].suit && w[1].rank + 1 == w[0].rank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_deal() {
        let g = Game::new(4, 1, false);
        let counts: Vec<usize> = g.columns.iter().map(|c| c.len()).collect();
        assert_eq!(counts, vec![6, 6, 6, 6, 5, 5, 5, 5, 5, 5]);
        assert_eq!(g.stock.len(), 5);
        assert!(g.stock.iter().all(|r| r.len() == 10));
        for col in &g.columns {
            assert!(col.last().unwrap().face_up);
            assert!(col[..col.len() - 1].iter().all(|c| !c.face_up));
        }
        assert_eq!(g.all_cards().count(), 104);
    }

    #[test]
    fn deal_and_undo() {
        let mut g = Game::new(2, 42, false);
        assert!(g.play(Move::Deal));
        assert_eq!(g.stock.len(), 4);
        assert_eq!(g.columns[0].len(), 7);
        assert_eq!(g.moves, 1);
        assert!(g.undo());
        assert_eq!(g.stock.len(), 5);
        assert_eq!(g.moves, 0);
        assert!(g.redo());
        assert_eq!(g.stock.len(), 4);
    }

    #[test]
    fn full_run_goes_to_foundation() {
        let mut g = Game::new(1, 7, true);
        // Construit à la main une suite Roi → Dame… → As en colonne 0, sauf l'As.
        let mut id = 200u8;
        let mut mk = |rank| {
            id = id.wrapping_add(1);
            Card { id, rank, suit: crate::cards::Suit::Spades, face_up: true }
        };
        g.columns[0] = (2..=13).rev().map(&mut mk).collect();
        g.columns[1] = vec![mk(1)];
        assert!(g.play(Move::Cards { from: 1, index: 0, to: 0 }));
        assert_eq!(g.runs.len(), 1);
        assert!(g.columns[0].is_empty());
    }
}
