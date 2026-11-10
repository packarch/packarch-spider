//! Règles du FreeCell et de ses variantes (comme dans KPatience,
//! `freecell.cpp`).
//!
//! - 1 à 3 paquets de 52 cartes, toutes distribuées face visible dans 6 à 12
//!   colonnes ;
//! - 0 à 8 cellules libres, qui reçoivent une carte chacune ;
//! - 4 fondations par paquet, montées de l'As au Roi par couleur ;
//! - dans les colonnes, on descend en alternant rouge et noir (FreeCell), en
//!   suivant la couleur (Baker's Game…) ou au rang seul ;
//! - une colonne vide reçoit n'importe quelle carte, seulement un Roi, ou
//!   rien du tout ;
//! - on déplace plusieurs cartes à la fois si les cellules et colonnes vides
//!   le permettraient carte par carte : (cellules libres + 1) × 2^(colonnes vides).
//!
//! Les piles sont numérotées : colonnes, puis cellules, puis fondations.

use serde::{Deserialize, Serialize};

pub use crate::board::Move;
use crate::cards::{self, Card, Suit};

/// Remplissage d'une colonne vide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmptyFill {
    Any,
    Kings,
    None,
}

/// Construction des suites dans les colonnes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Build {
    AlternateColor,
    Suit,
    Rank,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub empty_fill: EmptyFill,
    pub build: Build,
    /// 0 à 8.
    pub cells: u8,
    /// 6 à 12.
    pub stacks: u8,
    /// 1 à 3.
    pub decks: u8,
}

impl Default for Options {
    fn default() -> Options {
        PRESETS[FREECELL].1
    }
}

const fn opts(empty_fill: EmptyFill, build: Build, cells: u8, stacks: u8) -> Options {
    Options { empty_fill, build, cells, stacks, decks: 1 }
}

/// Les variantes connues, dans l'ordre (et avec les numéros) de KPatience.
pub const PRESETS: [(&str, Options); 5] = [
    ("Baker's Game", opts(EmptyFill::Any, Build::Suit, 4, 8)),
    ("Eight Off", opts(EmptyFill::Kings, Build::Suit, 8, 8)),
    ("Forecell", opts(EmptyFill::Kings, Build::AlternateColor, 4, 8)),
    ("FreeCell", opts(EmptyFill::Any, Build::AlternateColor, 4, 8)),
    ("Seahaven Towers", opts(EmptyFill::Kings, Build::Suit, 4, 10)),
];
pub const FREECELL: usize = 3;
/// Numéro de la variante « personnalisée ».
pub const CUSTOM: usize = 5;
pub const VARIANTS: usize = 6;

impl Options {
    pub fn sanitized(mut self) -> Options {
        self.cells = self.cells.min(8);
        self.stacks = self.stacks.clamp(6, 12);
        self.decks = self.decks.clamp(1, 3);
        self
    }

    /// Le numéro de la variante (0 à 4), ou `CUSTOM`. Comme KPatience
    /// (`Freecell::matchVariant`), mais avec un seul paquet.
    pub fn variant(&self) -> usize {
        PRESETS.iter().position(|(_, o)| o == self).unwrap_or(CUSTOM)
    }

    pub fn name(&self) -> &'static str {
        variant_name(self.variant())
    }
}

/// Le nom complet, avec le jeu (titre de la fenêtre, statistiques, donnes
/// numérotées) ; le menu des variantes garde les noms courts de `PRESETS`.
pub fn variant_name(variant: usize) -> &'static str {
    match variant {
        0 => "FreeCell (Baker's Game)",
        1 => "FreeCell (Eight Off)",
        2 => "FreeCell (Forecell)",
        3 => "FreeCell (classique)",
        4 => "FreeCell (Seahaven Towers)",
        _ => "FreeCell (personnalisé)",
    }
}

/// Les règles, sur des piles de cartes codées ou non (le jeu et l'outil de
/// résolution utilisent les mêmes fonctions, pour être toujours d'accord).
pub trait Code: Copy {
    /// Rang (1 à 13) dans les 4 bits du bas, couleur (0 à 3) au-dessus.
    fn code(self) -> u8;
}

impl Code for u8 {
    fn code(self) -> u8 {
        self
    }
}

impl Code for Card {
    fn code(self) -> u8 {
        self.rank | ((self.suit as u8) << 4)
    }
}

impl Code for &Card {
    fn code(self) -> u8 {
        (*self).code()
    }
}

pub fn rank(c: u8) -> u8 {
    c & 15
}

pub fn suit(c: u8) -> u8 {
    (c >> 4) & 3
}

pub fn is_red(c: u8) -> bool {
    matches!(suit(c), 1 | 2)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PileKind {
    Tableau,
    Cell,
    Foundation,
}

#[derive(Clone, Copy, Debug)]
pub struct Rules {
    pub opts: Options,
}

impl Rules {
    pub fn stacks(&self) -> usize {
        self.opts.stacks as usize
    }

    pub fn cells(&self) -> usize {
        self.opts.cells as usize
    }

    pub fn foundations(&self) -> usize {
        4 * self.opts.decks as usize
    }

    pub fn piles(&self) -> usize {
        self.stacks() + self.cells() + self.foundations()
    }

    pub fn first_cell(&self) -> usize {
        self.stacks()
    }

    pub fn first_foundation(&self) -> usize {
        self.stacks() + self.cells()
    }

    pub fn total_cards(&self) -> usize {
        52 * self.opts.decks as usize
    }

    pub fn kind(&self, p: usize) -> PileKind {
        if p < self.stacks() {
            PileKind::Tableau
        } else if p < self.first_foundation() {
            PileKind::Cell
        } else {
            PileKind::Foundation
        }
    }

    /// `card` peut-elle être posée sur `below` dans une colonne ?
    pub fn builds_on(&self, card: u8, below: u8) -> bool {
        rank(card) + 1 == rank(below)
            && match self.opts.build {
                Build::AlternateColor => is_red(card) != is_red(below),
                Build::Suit => suit(card) == suit(below),
                Build::Rank => true,
            }
    }

    pub fn is_sequence<C: Code>(&self, cards: &[C]) -> bool {
        cards.windows(2).all(|w| self.builds_on(w[1].code(), w[0].code()))
    }

    pub fn free_cells<C>(&self, piles: &[Vec<C>]) -> usize {
        piles[self.first_cell()..self.first_foundation()].iter().filter(|p| p.is_empty()).count()
    }

    /// Les colonnes vides (sauf `except`) utilisables pour un déplacement.
    pub fn free_stacks<C>(&self, piles: &[Vec<C>], except: usize) -> usize {
        if self.opts.empty_fill != EmptyFill::Any {
            return 0;
        }
        piles[..self.stacks()].iter().enumerate().filter(|(i, p)| *i != except && p.is_empty()).count()
    }

    /// Le nombre de cartes qu'on peut déplacer d'un coup vers `to`
    /// (`Freecell::canPutStore`).
    pub fn capacity<C>(&self, piles: &[Vec<C>], to: usize) -> usize {
        (self.free_cells(piles) + 1) << self.free_stacks(piles, to)
    }

    /// Les cartes de `p` à partir de `index` peuvent-elles être prises ?
    pub fn can_pick<C: Code>(&self, piles: &[Vec<C>], p: usize, index: usize) -> bool {
        let pile = &piles[p];
        if index >= pile.len() {
            return false;
        }
        match self.kind(p) {
            PileKind::Tableau => self.is_sequence(&pile[index..]),
            PileKind::Cell => index + 1 == pile.len(),
            PileKind::Foundation => false,
        }
    }

    /// Les cartes de `from` à partir de `index` peuvent-elles aller sur `to` ?
    pub fn can_drop<C: Code>(&self, piles: &[Vec<C>], from: usize, index: usize, to: usize) -> bool {
        let moving = &piles[from][index..];
        let Some(first) = moving.first().map(|c| c.code()) else { return false };
        let n = moving.len();
        let top = piles[to].last().map(|c| c.code());
        match self.kind(to) {
            PileKind::Tableau => {
                n <= self.capacity(piles, to)
                    && match top {
                        None => match self.opts.empty_fill {
                            EmptyFill::Any => true,
                            EmptyFill::Kings => rank(first) == 13,
                            EmptyFill::None => false,
                        },
                        Some(t) => self.builds_on(first, t),
                    }
            }
            PileKind::Cell => n == 1 && top.is_none(),
            PileKind::Foundation => {
                n == 1
                    && match top {
                        None => rank(first) == 1,
                        Some(t) => suit(t) == suit(first) && rank(t) + 1 == rank(first),
                    }
            }
        }
    }

    pub fn is_legal<C: Code>(&self, piles: &[Vec<C>], from: usize, index: usize, to: usize) -> bool {
        from != to && from < piles.len() && to < piles.len() && self.can_pick(piles, from, index) && self.can_drop(piles, from, index, to)
    }

    /// La fondation qui accepte `card` : celle de sa couleur, ou la première
    /// vide pour un As (comme `DealerScene::tryAutomaticMove`).
    pub fn foundation_for<C: Code>(&self, piles: &[Vec<C>], card: u8) -> Option<usize> {
        (self.first_foundation()..self.piles()).find(|&f| match piles[f].last().map(|c| c.code()) {
            None => rank(card) == 1,
            Some(t) => suit(t) == suit(card) && rank(t) + 1 == rank(card),
        })
    }

    /// Hauteur garantie des fondations de la couleur `s` : la plus basse de
    /// ses `decks` exemplaires (0 pour celles qui n'ont pas commencé).
    fn out_height<C: Code>(&self, piles: &[Vec<C>], s: u8) -> u8 {
        let mut heights: Vec<u8> = piles[self.first_foundation()..]
            .iter()
            .filter_map(|p| p.last().map(|c| c.code()))
            .filter(|&t| suit(t) == s)
            .map(rank)
            .collect();
        if heights.len() < self.opts.decks as usize {
            return 0;
        }
        heights.sort_unstable();
        heights[0]
    }

    /// Peut-on sortir `card` sans risque (`FreecellSolver::good_automove`,
    /// règle de Raymond) ? Avec la construction au rang seul, on attend que
    /// toutes les autres couleurs soient sorties jusqu'au rang inférieur.
    pub fn safe_to_drop<C: Code>(&self, piles: &[Vec<C>], card: u8) -> bool {
        let r = rank(card);
        if r <= 2 || self.opts.build == Build::Suit {
            return true;
        }
        let s = suit(card);
        let h = |t: u8| self.out_height(piles, t);
        if self.opts.build == Build::Rank {
            return (0..4).filter(|&t| t != s).all(|t| h(t) + 1 >= r);
        }
        let red = is_red(card);
        let opposite: Vec<u8> = (0..4).filter(|&t| is_red(t << 4) != red).collect();
        let same_other = (0..4).find(|&t| t != s && is_red(t << 4) == red).expect("deux couleurs par teinte");
        if opposite.iter().all(|&t| h(t) + 1 >= r) {
            return true;
        }
        // Règle de Raymond.
        opposite.iter().all(|&t| h(t) + 2 >= r) && h(same_other) + 3 >= r
    }

    /// Le premier coup vers les fondations qui ne risque rien (colonnes, puis
    /// cellules), pour le dépôt automatique.
    pub fn autodrop<C: Code>(&self, piles: &[Vec<C>]) -> Option<Move> {
        for from in 0..self.first_foundation() {
            let Some(card) = piles[from].last().map(|c| c.code()) else { continue };
            if let Some(to) = self.foundation_for(piles, card) {
                if self.safe_to_drop(piles, card) {
                    return Some(Move::Cards { from, index: piles[from].len() - 1, to });
                }
            }
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    piles: Vec<Vec<Card>>,
    moves: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub options: Options,
    pub deal_number: u32,
    /// Colonnes, cellules, fondations ; chaque pile de la carte du fond à
    /// celle du dessus.
    pub piles: Vec<Vec<Card>>,
    pub moves: u32,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl Game {
    /// Nouvelle donne, distribuée comme dans KPatience (`Freecell::restart`).
    pub fn new(options: Options, deal_number: u32) -> Game {
        let options = options.sanitized();
        let rules = Rules { opts: options };
        let deck = cards::deck(options.decks, &Suit::ALL);
        let mut pile = cards::shuffled(&deck, deal_number);
        let mut piles: Vec<Vec<Card>> = vec![Vec::new(); rules.piles()];
        let mut take = || {
            let mut c = pile.pop();
            if let Some(c) = c.as_mut() {
                c.face_up = true;
            }
            c
        };
        // Cellules remplies d'avance dans certaines variantes.
        let prefill = match options.variant() {
            4 => 2,
            1 | 2 => 4,
            _ => 0,
        };
        for i in 0..prefill.min(rules.cells()) {
            piles[rules.first_cell() + i].push(take().expect("paquet complet"));
        }
        let mut column = 0;
        while let Some(c) = take() {
            piles[column].push(c);
            column = (column + 1) % rules.stacks();
        }
        Game { options, deal_number, piles, moves: 0, undo: Vec::new(), redo: Vec::new() }
    }

    pub fn rules(&self) -> Rules {
        Rules { opts: self.options }
    }

    pub fn is_won(&self) -> bool {
        let r = self.rules();
        self.piles[r.first_foundation()..].iter().map(Vec::len).sum::<usize>() == r.total_cards()
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
        self.rules().autodrop(&self.piles)
    }

    /// Double-clic (`Freecell::tryAutomaticMove`) : la carte du dessus va
    /// sur une fondation, sinon dans une cellule libre ; une suite va sur une
    /// autre colonne (une carte, plutôt qu'une colonne vide).
    pub fn auto_move(&self, from: usize, index: usize) -> Option<Move> {
        if !self.can_pick(from, index) {
            return None;
        }
        let r = self.rules();
        let top = index + 1 == self.piles[from].len();
        let mv = |to| Move::Cards { from, index, to };
        if top {
            if let Some(to) = r.foundation_for(&self.piles, self.piles[from][index].code()) {
                return Some(mv(to));
            }
            if r.kind(from) == PileKind::Tableau {
                if let Some(to) = (r.first_cell()..r.first_foundation()).find(|&c| self.piles[c].is_empty()) {
                    return Some(mv(to));
                }
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
    /// connaît pas de chemin gagnant : vers les fondations et les colonnes
    /// (sauf ceux qui ne changent rien) ; vers les cellules s'il n'y a rien
    /// d'autre.
    pub fn sensible_moves(&self) -> Vec<Move> {
        let r = self.rules();
        let mut result = Vec::new();
        let mut to_cells = Vec::new();
        for from in 0..r.first_foundation() {
            let pile = &self.piles[from];
            for index in 0..pile.len() {
                if !r.can_pick(&self.piles, from, index) {
                    continue;
                }
                // Suite déjà posée sur une carte qui lui convient.
                let placed = r.kind(from) == PileKind::Tableau && index > 0 && r.builds_on(pile[index].code(), pile[index - 1].code());
                for to in 0..r.piles() {
                    if to == from || !r.can_drop(&self.piles, from, index, to) {
                        continue;
                    }
                    let m = Move::Cards { from, index, to };
                    match r.kind(to) {
                        PileKind::Foundation => result.push(m),
                        PileKind::Cell if r.kind(from) == PileKind::Tableau => to_cells.push(m),
                        PileKind::Cell => {}
                        PileKind::Tableau => {
                            let to_empty = self.piles[to].is_empty();
                            let whole = r.kind(from) == PileKind::Tableau && index == 0;
                            if !(to_empty && whole) && !(placed && !to_empty) {
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

    /// Toutes les cartes du jeu.
    #[cfg(test)]
    pub fn all_cards(&self) -> impl Iterator<Item = &Card> {
        self.piles.iter().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(rank: u8, suit: Suit, id: u8) -> Card {
        Card { id, rank, suit, face_up: true }
    }

    #[test]
    fn variant_names_show_the_game() {
        for v in 0..VARIANTS {
            assert!(variant_name(v).starts_with("FreeCell ("), "{}", variant_name(v));
        }
        assert!(variant_name(1).contains(PRESETS[1].0));
    }

    #[test]
    fn initial_deal() {
        let g = Game::new(Options::default(), 1);
        let counts: Vec<usize> = g.piles[..8].iter().map(Vec::len).collect();
        assert_eq!(counts, vec![7, 7, 7, 7, 6, 6, 6, 6]);
        assert_eq!(g.piles.len(), 8 + 4 + 4);
        assert!(g.all_cards().all(|c| c.face_up));
        let mut ids: Vec<u8> = g.all_cards().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..52).collect::<Vec<u8>>());
    }

    #[test]
    fn variants_prefill_cells() {
        let eight_off = Game::new(PRESETS[1].1, 5);
        assert!(eight_off.piles[8..12].iter().all(|p| p.len() == 1));
        assert!(eight_off.piles[12..16].iter().all(Vec::is_empty));
        let seahaven = Game::new(PRESETS[4].1, 5);
        assert_eq!(seahaven.piles[10..14].iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 1, 0, 0]);
        assert_eq!(seahaven.piles[..10].iter().map(Vec::len).sum::<usize>(), 50);
        let big = Game::new(Options { decks: 3, stacks: 12, cells: 8, ..Options::default() }, 9);
        assert_eq!(big.all_cards().count(), 156);
        assert_eq!(big.piles.len(), 12 + 8 + 12);
    }

    #[test]
    fn supermove_capacity() {
        let mut g = Game::new(Options::default(), 1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        // 9♠ 8♥ 7♣ 6♦ 5♠ en colonne 0, 10♥ en colonne 1, 6 colonnes vides.
        g.piles[0] = vec![
            card(9, Suit::Spades, 1),
            card(8, Suit::Hearts, 2),
            card(7, Suit::Clubs, 3),
            card(6, Suit::Diamonds, 4),
            card(5, Suit::Spades, 5),
        ];
        g.piles[1] = vec![card(10, Suit::Hearts, 6)];
        let r = g.rules();
        // 4 cellules libres, 6 colonnes vides (hors destination) : (4+1) << 6.
        assert_eq!(r.capacity(&g.piles, 1), 5 << 6);
        assert!(g.is_legal(Move::Cards { from: 0, index: 0, to: 1 }));
        // Plus de cellules ni de colonnes vides : une carte à la fois.
        for i in 2..8 {
            g.piles[i] = vec![card(13, Suit::Clubs, 10 + i as u8)];
        }
        for i in 8..12 {
            g.piles[i] = vec![card(12, Suit::Clubs, 20 + i as u8)];
        }
        assert_eq!(r.capacity(&g.piles, 1), 1);
        assert!(!g.is_legal(Move::Cards { from: 0, index: 0, to: 1 }));
        // Une seule carte : 6♦ ne va pas sur 10♥.
        assert!(!g.is_legal(Move::Cards { from: 0, index: 3, to: 1 }));
        // Les cellules et colonnes pleines ne reçoivent rien de plus.
        assert!(!g.is_legal(Move::Cards { from: 0, index: 4, to: 8 }));
    }

    #[test]
    fn foundations_and_undo() {
        let mut g = Game::new(Options::default(), 1);
        for p in g.piles.iter_mut() {
            p.clear();
        }
        g.piles[0] = vec![card(2, Suit::Hearts, 1), card(1, Suit::Hearts, 2)];
        let m = g.autodrop().expect("l'As sort");
        assert_eq!(m, Move::Cards { from: 0, index: 1, to: 12 });
        assert!(g.play(m));
        assert_eq!(g.autodrop(), Some(Move::Cards { from: 0, index: 0, to: 12 }));
        assert!(g.undo());
        assert_eq!(g.piles[0].len(), 2);
        assert!(g.redo());
        assert_eq!(g.piles[12].len(), 1);
    }

    #[test]
    fn raymond_rule() {
        let r = Rules { opts: Options::default() };
        let mut piles: Vec<Vec<u8>> = vec![Vec::new(); 16];
        let c = |rank: u8, s: Suit| rank | ((s as u8) << 4);
        // Fondations : ♣ 4, ♦ 3, ♥ 3, ♠ 4.
        piles[12] = vec![c(4, Suit::Clubs)];
        piles[13] = vec![c(3, Suit::Diamonds)];
        piles[14] = vec![c(3, Suit::Hearts)];
        piles[15] = vec![c(4, Suit::Spades)];
        // 4♦ : les 3 noirs sont sortis.
        assert!(r.safe_to_drop(&piles, c(4, Suit::Diamonds)));
        // 5♣ : les 4 rouges ne sont pas sortis, mais les 3 rouges et le 2♠ si.
        assert!(r.safe_to_drop(&piles, c(5, Suit::Clubs)));
        // 6♣ (5♣ sorti) : les 5 rouges manquent, et les 4 rouges aussi.
        piles[12] = vec![c(5, Suit::Clubs)];
        assert!(!r.safe_to_drop(&piles, c(6, Suit::Clubs)));
    }
}
