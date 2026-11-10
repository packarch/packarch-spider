//! Règles de Château (Castle) et de ses variantes, comme dans KPatience
//! (`castle.cpp`).
//!
//! - 52 cartes distribuées en 6 à 10 colonnes, selon la variante : les As
//!   posés d'avance sur les fondations, et même (Citadelle, Rois exilés)
//!   chaque carte qui les suit pendant la distribution ; colonnes face
//!   visible, face cachée ou une rangée sur deux (la dernière rangée
//!   toujours visible) ;
//! - 0 à 4 réserves, qui reçoivent une carte chacune ;
//! - 4 fondations, de l'As au Roi par couleur ;
//! - les mêmes règles qu'un FreeCell (`freecell::Rules` : colonnes, réserves
//!   = cellules, fondations ; `Castle::canPutStore` = `Freecell::canPutStore`),
//!   et le même outil de résolution (KPatience les confie tous deux à
//!   Freecell Solver).
//!
//! Piles : colonnes, réserves, fondations (comme au FreeCell).

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

pub use crate::board::{Move, Outcome};
use crate::cards::{self, Card, Suit};
use crate::freecell::{self, Build, Code, EmptyFill, HIDDEN, PileKind, Rules};
use crate::freecell_solver;

/// Les cartes des colonnes (`m_stackFaceup`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Faces {
    Down,
    Up,
    /// Une rangée sur deux face visible.
    Alternate,
}

/// Les fondations à la distribution (`m_foundation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prefill {
    None,
    /// Les As, posés d'avance.
    Aces,
    /// Les As, puis chaque carte qui les suit, pendant la distribution.
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub empty_fill: EmptyFill,
    pub build: Build,
    /// 0 à 4.
    pub reserves: u8,
    /// 6 à 10.
    pub stacks: u8,
    pub faces: Faces,
    pub prefill: Prefill,
    /// Disposition « moderne » : toutes les colonnes s'étalent vers la droite
    /// (`m_layout`, sans effet sur les règles).
    pub modern: bool,
}

impl Default for Options {
    fn default() -> Options {
        PRESETS[0].1
    }
}

const fn opts(empty_fill: EmptyFill, reserves: u8, prefill: Prefill) -> Options {
    Options { empty_fill, build: Build::Rank, reserves, stacks: 8, faces: Faces::Up, prefill, modern: false }
}

/// Les variantes connues, dans l'ordre de KPatience (`Castle::setOptions`).
pub const PRESETS: [(&str, Options); 6] = [
    ("Château assiégé", opts(EmptyFill::Any, 0, Prefill::Aces)),
    ("Citadelle", opts(EmptyFill::Any, 0, Prefill::Any)),
    ("Rois exilés", opts(EmptyFill::Kings, 0, Prefill::Any)),
    ("Rues et allées", opts(EmptyFill::Any, 0, Prefill::None)),
    ("Engin de siège", opts(EmptyFill::Any, 1, Prefill::Aces)),
    ("Forteresse", opts(EmptyFill::Any, 1, Prefill::None)),
];
/// Numéro de la variante « personnalisée ».
pub const CUSTOM: usize = 6;
pub const VARIANTS: usize = 7;

impl Options {
    pub fn sanitized(mut self) -> Options {
        self.reserves = self.reserves.min(4);
        self.stacks = self.stacks.clamp(6, 10);
        self
    }

    /// `Castle::matchVariant` : les faces des colonnes et la disposition ne
    /// comptent pas.
    pub fn variant(&self) -> usize {
        PRESETS.iter().position(|(_, o)| Options { faces: self.faces, modern: self.modern, ..*o } == *self).unwrap_or(CUSTOM)
    }

    pub fn rules(&self) -> Rules {
        Rules {
            opts: freecell::Options { empty_fill: self.empty_fill, build: self.build, cells: self.reserves, stacks: self.stacks, decks: 1 },
        }
    }
}

/// Le nom complet (titre, statistiques, donnes numérotées).
pub fn variant_name(variant: usize) -> &'static str {
    match variant {
        0 => "Château (Château assiégé)",
        1 => "Château (Citadelle)",
        2 => "Château (Rois exilés)",
        3 => "Château (Rues et allées)",
        4 => "Château (Engin de siège)",
        5 => "Château (Forteresse)",
        _ => "Château (personnalisé)",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

/// `castle` (les options) distingue une partie de Château des autres jeux
/// dans la partie enregistrée.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub castle: Options,
    pub deal_number: u32,
    /// Colonnes, réserves, fondations.
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Castle::restart`).
    pub fn new(options: Options, deal_number: u32) -> Game {
        let options = options.sanitized();
        let r = options.rules();
        let stacks = r.stacks();
        let f0 = r.first_foundation();
        let deck = cards::deck(1, &Suit::ALL);
        let mut list = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); r.piles()];
        // Les As d'avance, dans l'ordre de la liste.
        let mut fdx = 0;
        if options.prefill != Prefill::None {
            let mut i = 0;
            while i < list.len() {
                if list[i].rank == 1 {
                    piles[f0 + fdx].push(Card { face_up: true, ..list.remove(i) });
                    fdx += 1;
                } else {
                    i += 1;
                }
            }
        }
        let mut column = 0;
        let mut alternate = false;
        while let Some(&card) = list.last() {
            if options.prefill == Prefill::Any {
                let fits = |f: &Vec<Card>| f.last().is_some_and(|t| t.suit == card.suit && t.rank + 1 == card.rank);
                if let Some(i) = (0..fdx).find(|&i| fits(&piles[f0 + i])) {
                    list.pop();
                    piles[f0 + i].push(Card { face_up: true, ..card });
                    // KPatience passe aussi à la colonne suivante.
                    column = (column + 1) % stacks;
                    continue;
                }
            }
            let face_up = options.faces == Faces::Up || list.len() <= stacks || (options.faces == Faces::Alternate && alternate);
            list.pop();
            piles[column].push(Card { face_up, ..card });
            column = (column + 1) % stacks;
            if column == 0 {
                alternate = !alternate;
            }
        }
        // La carte du dessus se retourne (`setAutoTurnTop`).
        for pile in piles.iter_mut().take(stacks) {
            if let Some(top) = pile.last_mut() {
                top.face_up = true;
            }
        }
        Game { castle: options, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn rules(&self) -> Rules {
        self.castle.rules()
    }

    pub fn is_won(&self) -> bool {
        self.piles[self.rules().first_foundation()..].iter().all(|p| p.len() == 13)
    }

    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        p < self.piles.len() && self.rules().can_pick(&self.piles, p, index)
    }

    pub fn is_legal(&self, m: Move) -> bool {
        match m {
            Move::Deal => false,
            Move::Cards { from, index, to } => self.rules().is_legal(&self.piles, from, index, to),
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

    /// Dépôt automatique : les sorties sans risque, comme au FreeCell.
    pub fn autodrop(&self) -> Option<Move> {
        self.rules().autodrop(&self.piles)
    }

    /// Double-clic (`Castle::tryAutomaticMove`) : la carte du dessus va sur
    /// une fondation, sinon dans une réserve libre ; une suite va sur une
    /// autre colonne (une carte plutôt qu'une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let r = self.rules();
        let mv = |to| Move::Cards { from, index, to };
        if index + 1 == self.piles[from].len() {
            if let Some(to) = r.foundation_for(&self.piles, self.piles[from][index].code()) {
                return Some(mv(to));
            }
            if r.kind(from) == PileKind::Tableau
                && let Some(to) = (r.first_cell()..r.first_foundation()).find(|&c| self.piles[c].is_empty())
            {
                return Some(mv(to));
            }
            return None;
        }
        let legal = |to: &usize| *to != from && r.can_drop(&self.piles, from, index, *to);
        (0..r.stacks())
            .filter(legal)
            .find(|&to| !self.piles[to].is_empty())
            .or_else(|| (0..r.stacks()).filter(legal).find(|_| index > 0))
            .map(mv)
    }

    /// Les coups utiles, pour les conseils quand l'outil de résolution ne
    /// connaît pas de chemin gagnant (vers les réserves s'il n'y a rien
    /// d'autre).
    pub fn sensible_moves(&self) -> Vec<Move> {
        let r = self.rules();
        let mut result = Vec::new();
        let mut to_cells = Vec::new();
        for from in 0..r.first_foundation() {
            let pile = &self.piles[from];
            for index in 0..pile.len() {
                if !self.can_pick(from, index) {
                    continue;
                }
                let tableau = r.kind(from) == PileKind::Tableau;
                let placed = tableau && index > 0 && r.builds_on(pile[index].code(), pile[index - 1].code());
                for to in 0..r.piles() {
                    let m = Move::Cards { from, index, to };
                    if to == from || !self.is_legal(m) {
                        continue;
                    }
                    match r.kind(to) {
                        PileKind::Foundation => result.push(m),
                        PileKind::Cell if tableau => to_cells.push(m),
                        PileKind::Cell => {}
                        PileKind::Tableau => {
                            let to_empty = self.piles[to].is_empty();
                            if !((to_empty && tableau && index == 0) || (placed && !to_empty)) {
                                result.push(m);
                            }
                        }
                    }
                }
            }
        }
        if result.is_empty() {
            to_cells.dedup_by_key(|m| match m {
                Move::Cards { from, .. } => *from,
                Move::Deal => usize::MAX,
            });
            result = to_cells;
        }
        result
    }
}

/// Cherche une suite de coups gagnante : l'outil de résolution du FreeCell,
/// qui connaît les cartes cachées (comme Freecell Solver dans KPatience).
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    let r = game.rules();
    let piles: Vec<Vec<u8>> = game
        .piles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if r.kind(i) == PileKind::Foundation {
                p.last().map(|c| c.code()).into_iter().collect()
            } else {
                p.iter().map(|c| c.code() | if c.face_up { 0 } else { HIDDEN }).collect()
            }
        })
        .collect();
    freecell_solver::solve_piles(r, piles, stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_ids(g: &Game) -> Vec<u8> {
        let mut ids: Vec<u8> = g.piles.iter().flatten().map(|c| c.id).collect();
        ids.sort();
        ids
    }

    #[test]
    fn deals() {
        for (v, (_, o)) in PRESETS.iter().enumerate() {
            let g = Game::new(*o, 7);
            let r = g.rules();
            assert_eq!(g.piles.len(), r.piles(), "variante {v}");
            assert_eq!(all_ids(&g), (0..52).collect::<Vec<u8>>(), "variante {v}");
            let f = &g.piles[r.first_foundation()..];
            match o.prefill {
                Prefill::None => assert!(f.iter().all(Vec::is_empty)),
                Prefill::Aces => assert!(f.iter().all(|p| p.len() == 1 && p[0].rank == 1)),
                Prefill::Any => assert!(f.iter().all(|p| !p.is_empty() && p[0].rank == 1)),
            }
            assert!(g.piles[r.first_cell()..r.first_foundation()].iter().all(Vec::is_empty));
            assert!(g.piles[..r.stacks()].iter().flatten().all(|c| c.face_up));
        }
        // Rues et allées : 52 cartes sur 8 colonnes, la colonne 0 reçoit la
        // dernière carte de la liste.
        let list = cards::shuffled(&cards::deck(1, &Suit::ALL), 7);
        let g = Game::new(PRESETS[3].1, 7);
        assert_eq!(g.piles[0][0].id, list[51].id);
        assert_eq!(g.piles[1][0].id, list[50].id);
        assert_eq!(g.piles[..8].iter().map(Vec::len).collect::<Vec<_>>(), vec![7, 7, 7, 7, 6, 6, 6, 6]);
    }

    #[test]
    fn faces() {
        let o = Options { faces: Faces::Down, ..PRESETS[3].1 };
        let g = Game::new(o, 3);
        for col in &g.piles[..8] {
            assert!(col.last().unwrap().face_up);
            assert!(col[..col.len() - 1].iter().all(|c| !c.face_up));
        }
        assert_eq!(o.variant(), 3, "les faces ne changent pas la variante");
        let o = Options { faces: Faces::Alternate, ..PRESETS[3].1 };
        let g = Game::new(o, 3);
        // Rangées 0, 2, 4 cachées ; 1, 3, 5 visibles ; la dernière visible.
        assert!(!g.piles[0][0].face_up && g.piles[0][1].face_up && !g.piles[0][2].face_up);
        let custom = Options { build: Build::AlternateColor, ..PRESETS[0].1 };
        assert_eq!(custom.variant(), CUSTOM);
    }

    #[test]
    fn hidden_cards_move_only_once_turned() {
        let o = Options { faces: Faces::Down, ..PRESETS[3].1 };
        let mut g = Game::new(o, 3);
        let n = g.piles[0].len();
        assert!(!g.can_pick(0, n - 2));
        // Vider la colonne 0 vers une réserve n'existe pas ici : on joue sa
        // carte du dessus sur une colonne vide créée à la main.
        g.piles[7].clear();
        assert!(g.play(Move::Cards { from: 0, index: n - 1, to: 7 }));
        assert!(g.piles[0].last().unwrap().face_up);
    }

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        for (v, (_, o)) in PRESETS.iter().enumerate() {
            let mut solved = 0;
            for deal in 1..=8 {
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
            assert!(solved >= 1, "variante {v} : {solved} donnes résolues sur 8");
        }
    }
}
