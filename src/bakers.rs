//! Règles de Treize à la douzaine (Baker's Dozen) et de ses variantes, comme
//! dans KPatience (`bakersdozen.cpp`).
//!
//! - 52 cartes en 13 colonnes de 4 ; dans chaque colonne, les Rois passent
//!   au fond ; « Châteaux en Espagne » : les 3 premières rangées (et les
//!   Rois) face cachée ;
//! - 4 fondations, de l'As au Roi par couleur ;
//! - pas de cellule : c'est un FreeCell à 13 colonnes et 0 cellule, dont il
//!   réutilise les règles (`freecell::Rules`) et l'outil de résolution
//!   (KPatience les confie tous deux à Freecell Solver) ;
//! - une colonne vide reçoit n'importe quelle carte, un Roi seulement ou
//!   rien ; les suites descendent au rang seul, dans la même couleur ou en
//!   alternant rouge et noir ; une suite se déplace d'un bloc jusqu'à
//!   2^(colonnes vides) cartes (si elles reçoivent n'importe quelle carte).
//!
//! Piles : colonnes 0 à 12, fondations 13 à 16.

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

pub use crate::board::{Move, Outcome};
use crate::cards::{self, Card, Suit};
use crate::freecell::{self, Build, Code, EmptyFill, HIDDEN, Rules};
use crate::freecell_solver;

pub const STORES: usize = 13;
pub const FOUNDATION: usize = 13;
pub const PILES: usize = 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub empty_fill: EmptyFill,
    /// Les 3 premières rangées (et les Rois) face cachée.
    pub face_down: bool,
    pub build: Build,
}

impl Default for Options {
    fn default() -> Options {
        PRESETS[0].1
    }
}

const fn opts(empty_fill: EmptyFill, face_down: bool, build: Build) -> Options {
    Options { empty_fill, face_down, build }
}

/// Les variantes connues, dans l'ordre de KPatience (`BakersDozen::setOptions`).
pub const PRESETS: [(&str, Options); 4] = [
    ("Treize à la douzaine", opts(EmptyFill::None, false, Build::Rank)),
    ("Patience espagnole", opts(EmptyFill::Any, false, Build::Rank)),
    ("Châteaux en Espagne", opts(EmptyFill::Any, true, Build::AlternateColor)),
    ("Solitaire portugais", opts(EmptyFill::Kings, false, Build::Rank)),
];
/// Numéro de la variante « personnalisée ».
pub const CUSTOM: usize = 4;
pub const VARIANTS: usize = 5;

impl Options {
    /// `BakersDozen::matchVariant`.
    pub fn variant(&self) -> usize {
        PRESETS.iter().position(|(_, o)| o == self).unwrap_or(CUSTOM)
    }

    pub fn rules(&self) -> Rules {
        Rules { opts: freecell::Options { empty_fill: self.empty_fill, build: self.build, cells: 0, stacks: STORES as u8, decks: 1 } }
    }
}

/// Le nom complet (titre, statistiques, donnes numérotées).
pub fn variant_name(variant: usize) -> &'static str {
    match variant {
        0 => "Treize à la douzaine",
        1 => "Treize à la douzaine (Patience espagnole)",
        2 => "Treize à la douzaine (Châteaux en Espagne)",
        3 => "Treize à la douzaine (Solitaire portugais)",
        _ => "Treize à la douzaine (personnalisé)",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `bakers` (les options) distingue une partie de Treize à la douzaine des
/// autres jeux dans la partie enregistrée (le FreeCell a `options`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub bakers: Options,
    pub deal_number: u32,
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience
    /// (`BakersDozen::restart`) : 4 rangées prises à la fin de la liste ;
    /// dans chaque colonne, chaque Roi (de bas en haut) échange sa place avec
    /// la première qui n'est pas déjà un Roi descendu.
    pub fn new(options: Options, deal_number: u32) -> Game {
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); PILES];
        for row in 0..4 {
            let face_up = !(options.face_down && row < 3);
            for pile in piles.iter_mut().take(STORES) {
                pile.push(Card { face_up, ..list.pop().expect("paquet complet") });
            }
        }
        for pile in piles.iter_mut().take(STORES) {
            let kings: Vec<u8> = pile.iter().filter(|c| c.rank == 13).map(|c| c.id).collect();
            for (counter, id) in kings.into_iter().enumerate() {
                let index = pile.iter().position(|c| c.id == id).expect("Roi");
                pile.swap(index, counter);
                if options.face_down {
                    pile[counter].face_up = false;
                }
            }
            // La carte du dessus se retourne (`setAutoTurnTop`).
            if let Some(top) = pile.last_mut() {
                top.face_up = true;
            }
        }
        Game { bakers: options, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn rules(&self) -> Rules {
        self.bakers.rules()
    }

    pub fn is_won(&self) -> bool {
        self.piles[FOUNDATION..].iter().all(|p| p.len() == 13)
    }

    /// `BakersDozen::checkRemove` : une suite de cartes visibles ; une carte
    /// seule si les colonnes vides ne reçoivent rien.
    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < STORES
            && (self.bakers.empty_fill != EmptyFill::None || index + 1 == self.piles[p].len())
            && self.rules().can_pick(&self.piles, p, index)
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => false,
            Move::Cards { from, index, to } => {
                from < PILES && to < PILES && self.can_pick(from, index) && self.rules().is_legal(&self.piles, from, index, to)
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

    /// Dépôt automatique : les sorties sans risque (règle de Raymond), comme
    /// au FreeCell.
    pub fn autodrop(&self) -> Option<Move> {
        self.rules().autodrop(&self.piles)
    }

    /// Double-clic : la carte du dessus va sur une fondation ; sinon la suite
    /// va sur une autre colonne (une carte plutôt qu'une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let r = self.rules();
        let mv = |to| Move::Cards { from, index, to };
        if index + 1 == self.piles[from].len()
            && let Some(to) = r.foundation_for(&self.piles, self.piles[from][index].code())
        {
            return Some(mv(to));
        }
        let legal = |to: &usize| *to != from && r.can_drop(&self.piles, from, index, *to);
        (0..STORES).filter(legal).find(|&to| !self.piles[to].is_empty()).or_else(|| (0..STORES).filter(legal).find(|_| index > 0)).map(mv)
    }

    /// Les coups utiles, pour les conseils quand l'outil de résolution ne
    /// connaît pas de chemin gagnant (sans les suites déjà bien posées ni les
    /// colonnes entières vers une colonne vide).
    pub fn sensible_moves(&self) -> Vec<Move> {
        let r = self.rules();
        let mut result = Vec::new();
        for from in 0..STORES {
            let pile = &self.piles[from];
            for index in 0..pile.len() {
                if !self.can_pick(from, index) {
                    continue;
                }
                let placed = index > 0 && r.builds_on(pile[index].code(), pile[index - 1].code());
                for to in 0..PILES {
                    let m = Move::Cards { from, index, to };
                    if to == from || !self.is_legal(m) {
                        continue;
                    }
                    let to_empty = to < STORES && self.piles[to].is_empty();
                    if to >= FOUNDATION || !((to_empty && index == 0) || (placed && !to_empty)) {
                        result.push(m);
                    }
                }
            }
        }
        result
    }
}

/// Cherche une suite de coups gagnante : l'outil de résolution du FreeCell,
/// qui connaît les cartes cachées (comme Freecell Solver dans KPatience).
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    let piles: Vec<Vec<u8>> = game
        .piles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i >= FOUNDATION {
                p.last().map(|c| c.code()).into_iter().collect()
            } else {
                p.iter().map(|c| c.code() | if c.face_up { 0 } else { HIDDEN }).collect()
            }
        })
        .collect();
    freecell_solver::solve_piles(game.rules(), piles, stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_deal() {
        for (_, o) in PRESETS {
            let g = Game::new(o, 1);
            let counts: Vec<usize> = g.piles.iter().map(Vec::len).collect();
            assert_eq!(counts, [vec![4; 13], vec![0; 4]].concat());
            let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
            ids.sort();
            assert_eq!(ids, (0..52).collect::<Vec<u8>>());
            for col in &g.piles[..STORES] {
                // Les Rois au fond, la carte du dessus visible.
                let kings = col.iter().filter(|c| c.rank == 13).count();
                assert!(col[..kings].iter().all(|c| c.rank == 13));
                assert!(col.last().unwrap().face_up);
                if !o.face_down {
                    assert!(col.iter().all(|c| c.face_up));
                }
            }
        }
        // Comme KPatience : la colonne 0 reçoit la dernière carte de la liste.
        let list = cards::shuffled(&cards::deck(1, &Suit::ALL), 1);
        let g = Game::new(PRESETS[0].1, 1);
        if g.piles[0].iter().all(|c| c.rank != 13) {
            assert_eq!(g.piles[0][0].id, list[51].id);
        }
    }

    #[test]
    fn kings_go_to_the_bottom_by_swapping() {
        // Recherche d'une donne où une colonne a un Roi au-dessus du fond.
        let o = PRESETS[0].1;
        let deck = cards::deck(1, &Suit::ALL);
        for deal in 1..50 {
            let mut list = cards::shuffled(&deck, deal);
            let mut cols: Vec<Vec<Card>> = vec![Vec::new(); STORES];
            for _ in 0..4 {
                for col in cols.iter_mut() {
                    col.push(list.pop().unwrap());
                }
            }
            let g = Game::new(o, deal);
            for (c, col) in cols.iter_mut().enumerate() {
                let mut counter = 0;
                for i in 0..4 {
                    if col[i].rank == 13 {
                        col.swap(i, counter);
                        counter += 1;
                    }
                }
                let ids: Vec<u8> = col.iter().map(|c| c.id).collect();
                let got: Vec<u8> = g.piles[c].iter().map(|c| c.id).collect();
                assert_eq!(ids, got, "donne {deal}, colonne {c}");
            }
        }
    }

    #[test]
    fn rules() {
        let card = |rank, suit, face_up| Card { id: 0, rank, suit, face_up };
        let mut g = Game::new(PRESETS[2].1, 1); // Châteaux en Espagne
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(4, Suit::Clubs, false), card(9, Suit::Hearts, true), card(8, Suit::Spades, true)];
        g.piles[1] = vec![card(10, Suit::Clubs, true)];
        for p in 2..STORES {
            g.piles[p] = vec![card(13, Suit::Diamonds, true)];
        }
        // Pas de colonne vide : une carte à la fois ; jamais une carte cachée.
        assert!(g.can_pick(0, 1) && !g.can_pick(0, 0));
        assert!(!g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        g.piles[12].clear();
        assert!(g.is_legal(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(g.play(Move::Cards { from: 0, index: 1, to: 1 }));
        assert!(g.piles[0][0].face_up, "la carte découverte se retourne");
        // Treize à la douzaine : colonnes vides interdites, une carte à la fois.
        let mut b = Game::new(PRESETS[0].1, 1);
        b.piles[0].clear();
        assert!(!(0..STORES).any(|from| from != 0 && b.is_legal(Move::Cards { from, index: b.piles[from].len() - 1, to: 0 })));
    }

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        for (v, (_, o)) in PRESETS.iter().enumerate() {
            let mut solved = 0;
            for deal in 1..=10 {
                let g = Game::new(*o, deal);
                if let Outcome::Winnable(moves) = solve(&g, &stop) {
                    let mut end = g.clone();
                    for (k, &m) in moves.iter().enumerate() {
                        assert!(end.play(m), "variante {v}, donne {deal}, coup {k} illégal : {m:?}");
                    }
                    assert!(end.is_won(), "variante {v}, donne {deal} : la solution ne gagne pas");
                    solved += 1;
                }
            }
            assert!(solved >= 3, "variante {v} : {solved} donnes résolues sur 10");
        }
    }
}
