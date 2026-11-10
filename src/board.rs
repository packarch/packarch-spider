//! Ce qui est commun aux jeux : les coups, la partie (Spider ou FreeCell)
//! vue par l'interface, les variantes et l'outil de résolution.

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::cards::Card;
use crate::freecell::{self, Code};
use crate::{aces, aces_solver, bakers, castle, clock, normandy, normandy_solver, fortyeight, fortyeight_solver, clock_solver, freecell_solver, golf, golf_solver, grandf, grandf_solver, klondike, klondike_solver, mod3, mod3_solver, simon, simon_solver, spider, spider_solver, yukon, yukon_solver};

/// Un coup du joueur (ou du solveur).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Move {
    /// Déplace les cartes de la pile `from` à partir de l'indice `index` sur
    /// la pile `to`.
    Cards { from: usize, index: usize, to: usize },
    /// Spider : distribue la rangée suivante de la réserve.
    Deal,
}

/// Résultat de l'outil de résolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Gagnable : la suite des coups, depuis la position donnée.
    Winnable(Vec<Move>),
    NotWinnable,
    /// Recherche trop large : impossible de conclure.
    Unknown,
    /// Arrêtée avant la fin (la position a changé).
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GameKind {
    #[default]
    Spider,
    FreeCell,
    Klondike,
    Clock,
    Simon,
    Golf,
    Yukon,
    Grandfather,
    Mod3,
    AcesUp,
    FortyEight,
    Bakers,
    Castle,
    Normandy,
}

/// Une variante, pour les statistiques et les donnes numérotées : le Spider
/// à 1 à 4 couleurs, une variante du FreeCell (0 à 4, ou personnalisée), le
/// Klondike en pioche 1 ou 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Spider(u8),
    FreeCell(usize),
    Klondike(u8),
    Clock,
    Simon,
    Golf,
    Yukon,
    Grandfather,
    Mod3,
    AcesUp,
    FortyEight,
    /// Treize à la douzaine : variante 0 à 3, ou personnalisée.
    Bakers(usize),
    /// Château : variante 0 à 5, ou personnalisée.
    Castle(usize),
    Normandy,
}

impl Variant {
    pub const ALL: [Variant; 33] = [
        Variant::Klondike(1),
        Variant::Klondike(3),
        Variant::Spider(1),
        Variant::Spider(2),
        Variant::Spider(3),
        Variant::Spider(4),
        Variant::FreeCell(0),
        Variant::FreeCell(1),
        Variant::FreeCell(2),
        Variant::FreeCell(3),
        Variant::FreeCell(4),
        Variant::FreeCell(freecell::CUSTOM),
        Variant::Clock,
        Variant::Simon,
        Variant::Golf,
        Variant::Yukon,
        Variant::Grandfather,
        Variant::Mod3,
        Variant::AcesUp,
        Variant::FortyEight,
        Variant::Bakers(0),
        Variant::Bakers(1),
        Variant::Bakers(2),
        Variant::Bakers(3),
        Variant::Bakers(bakers::CUSTOM),
        Variant::Castle(0),
        Variant::Castle(1),
        Variant::Castle(2),
        Variant::Castle(3),
        Variant::Castle(4),
        Variant::Castle(5),
        Variant::Castle(castle::CUSTOM),
        Variant::Normandy,
    ];

    pub fn name(self) -> String {
        match self {
            Variant::Spider(1) => "Spider (1 couleur)".into(),
            Variant::Spider(n) => format!("Spider ({n} couleurs)"),
            Variant::FreeCell(v) => freecell::variant_name(v).into(),
            Variant::Klondike(d) => format!("Klondike (pioche {d})"),
            Variant::Clock => "L'horloge du grand-père".into(),
            Variant::Simon => "Simon le simple".into(),
            Variant::Golf => "Golf".into(),
            Variant::Yukon => "Yukon".into(),
            Variant::Grandfather => "Grand-père".into(),
            Variant::Mod3 => "Mod3".into(),
            Variant::AcesUp => "Les as".into(),
            Variant::FortyEight => "Quarante & Huit".into(),
            Variant::Bakers(v) => bakers::variant_name(v).into(),
            Variant::Castle(v) => castle::variant_name(v).into(),
            Variant::Normandy => "Normandie".into(),
        }
    }
}

/// Rôle d'une pile, pour l'interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PileKind {
    Tableau,
    Cell,
    Foundation,
    /// Klondike : la pioche (cartes retournées du talon).
    Waste,
    /// Klondike : le talon.
    Stock,
}

/// La partie en cours. `untagged` : les parties de Spider enregistrées avant
/// l'arrivée du FreeCell se relisent telles quelles.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnyGame {
    Spider(spider::Game),
    FreeCell(freecell::Game),
    Klondike(klondike::Game),
    Clock(clock::Game),
    Simon(simon::Game),
    Golf(golf::Game),
    Yukon(yukon::Game),
    Grandfather(grandf::Game),
    Mod3(mod3::Game),
    AcesUp(aces::Game),
    FortyEight(fortyeight::Game),
    Bakers(bakers::Game),
    Castle(castle::Game),
    Normandy(normandy::Game),
}

impl AnyGame {
    pub fn kind(&self) -> GameKind {
        match self {
            AnyGame::Spider(_) => GameKind::Spider,
            AnyGame::FreeCell(_) => GameKind::FreeCell,
            AnyGame::Klondike(_) => GameKind::Klondike,
            AnyGame::Clock(_) => GameKind::Clock,
            AnyGame::Simon(_) => GameKind::Simon,
            AnyGame::Golf(_) => GameKind::Golf,
            AnyGame::Yukon(_) => GameKind::Yukon,
            AnyGame::Grandfather(_) => GameKind::Grandfather,
            AnyGame::Mod3(_) => GameKind::Mod3,
            AnyGame::AcesUp(_) => GameKind::AcesUp,
            AnyGame::FortyEight(_) => GameKind::FortyEight,
            AnyGame::Bakers(_) => GameKind::Bakers,
            AnyGame::Castle(_) => GameKind::Castle,
            AnyGame::Normandy(_) => GameKind::Normandy,
        }
    }

    pub fn variant(&self) -> Variant {
        match self {
            AnyGame::Spider(g) => Variant::Spider(g.suit_count),
            AnyGame::FreeCell(g) => Variant::FreeCell(g.options.variant()),
            AnyGame::Klondike(g) => Variant::Klondike(g.draw),
            AnyGame::Clock(_) => Variant::Clock,
            AnyGame::Simon(_) => Variant::Simon,
            AnyGame::Golf(_) => Variant::Golf,
            AnyGame::Yukon(_) => Variant::Yukon,
            AnyGame::Grandfather(_) => Variant::Grandfather,
            AnyGame::Mod3(_) => Variant::Mod3,
            AnyGame::AcesUp(_) => Variant::AcesUp,
            AnyGame::FortyEight(_) => Variant::FortyEight,
            AnyGame::Bakers(g) => Variant::Bakers(g.bakers.variant()),
            AnyGame::Castle(g) => Variant::Castle(g.castle.variant()),
            AnyGame::Normandy(_) => Variant::Normandy,
        }
    }

    pub fn title(&self) -> String {
        match self {
            AnyGame::Spider(g) => format!("Spider ({} couleur{})", g.suit_count, if g.suit_count > 1 { "s" } else { "" }),
            AnyGame::FreeCell(g) => g.options.name().into(),
            AnyGame::Klondike(_) | AnyGame::Clock(_) | AnyGame::Simon(_) | AnyGame::Golf(_) | AnyGame::Yukon(_) | AnyGame::Grandfather(_) | AnyGame::Mod3(_) | AnyGame::AcesUp(_) | AnyGame::FortyEight(_) | AnyGame::Bakers(_) | AnyGame::Castle(_) | AnyGame::Normandy(_) => self.variant().name(),
        }
    }

    /// La même donne, depuis le début.
    pub fn restarted(&self) -> AnyGame {
        match self {
            AnyGame::Spider(g) => AnyGame::Spider(spider::Game::new(g.suit_count, g.deal_number, g.stacks_face_up)),
            AnyGame::FreeCell(g) => AnyGame::FreeCell(freecell::Game::new(g.options, g.deal_number)),
            AnyGame::Klondike(g) => AnyGame::Klondike(klondike::Game::new(g.draw, g.deal_number)),
            AnyGame::Clock(g) => AnyGame::Clock(clock::Game::new(g.deal_number)),
            AnyGame::Simon(g) => AnyGame::Simon(simon::Game::new(g.deal_number)),
            AnyGame::Golf(g) => AnyGame::Golf(golf::Game::new(g.deal_number)),
            AnyGame::Yukon(g) => AnyGame::Yukon(yukon::Game::new(g.deal_number)),
            AnyGame::Grandfather(g) => AnyGame::Grandfather(grandf::Game::new(g.deal_number)),
            AnyGame::Mod3(g) => AnyGame::Mod3(mod3::Game::new(g.deal_number)),
            AnyGame::AcesUp(g) => AnyGame::AcesUp(aces::Game::new(g.deal_number)),
            AnyGame::FortyEight(g) => AnyGame::FortyEight(fortyeight::Game::new(g.deal_number)),
            AnyGame::Bakers(g) => AnyGame::Bakers(bakers::Game::new(g.bakers, g.deal_number)),
            AnyGame::Castle(g) => AnyGame::Castle(castle::Game::new(g.castle, g.deal_number)),
            AnyGame::Normandy(g) => AnyGame::Normandy(normandy::Game::new(g.deal_number)),
        }
    }

    pub fn deal_number(&self) -> u32 {
        match self {
            AnyGame::Spider(g) => g.deal_number,
            AnyGame::FreeCell(g) => g.deal_number,
            AnyGame::Klondike(g) => g.deal_number,
            AnyGame::Clock(g) => g.deal_number,
            AnyGame::Simon(g) => g.deal_number,
            AnyGame::Golf(g) => g.deal_number,
            AnyGame::Yukon(g) => g.deal_number,
            AnyGame::Grandfather(g) => g.deal_number,
            AnyGame::Mod3(g) => g.deal_number,
            AnyGame::AcesUp(g) => g.deal_number,
            AnyGame::FortyEight(g) => g.deal_number,
            AnyGame::Bakers(g) => g.deal_number,
            AnyGame::Castle(g) => g.deal_number,
            AnyGame::Normandy(g) => g.deal_number,
        }
    }

    pub fn moves(&self) -> u32 {
        match self {
            AnyGame::Spider(g) => g.moves,
            AnyGame::FreeCell(g) => g.moves,
            AnyGame::Klondike(g) => g.moves,
            AnyGame::Clock(g) => g.moves,
            AnyGame::Simon(g) => g.moves,
            AnyGame::Golf(g) => g.moves,
            AnyGame::Yukon(g) => g.moves,
            AnyGame::Grandfather(g) => g.moves,
            AnyGame::Mod3(g) => g.moves,
            AnyGame::AcesUp(g) => g.moves,
            AnyGame::FortyEight(g) => g.moves,
            AnyGame::Bakers(g) => g.moves,
            AnyGame::Castle(g) => g.moves,
            AnyGame::Normandy(g) => g.moves,
        }
    }

    pub fn is_won(&self) -> bool {
        match self {
            AnyGame::Spider(g) => g.is_won(),
            AnyGame::FreeCell(g) => g.is_won(),
            AnyGame::Klondike(g) => g.is_won(),
            AnyGame::Clock(g) => g.is_won(),
            AnyGame::Simon(g) => g.is_won(),
            AnyGame::Golf(g) => g.is_won(),
            AnyGame::Yukon(g) => g.is_won(),
            AnyGame::Grandfather(g) => g.is_won(),
            AnyGame::Mod3(g) => g.is_won(),
            AnyGame::AcesUp(g) => g.is_won(),
            AnyGame::FortyEight(g) => g.is_won(),
            AnyGame::Bakers(g) => g.is_won(),
            AnyGame::Castle(g) => g.is_won(),
            AnyGame::Normandy(g) => g.is_won(),
        }
    }

    pub fn can_undo(&self) -> bool {
        match self {
            AnyGame::Spider(g) => g.can_undo(),
            AnyGame::FreeCell(g) => g.can_undo(),
            AnyGame::Klondike(g) => g.can_undo(),
            AnyGame::Clock(g) => g.can_undo(),
            AnyGame::Simon(g) => g.can_undo(),
            AnyGame::Golf(g) => g.can_undo(),
            AnyGame::Yukon(g) => g.can_undo(),
            AnyGame::Grandfather(g) => g.can_undo(),
            AnyGame::Mod3(g) => g.can_undo(),
            AnyGame::AcesUp(g) => g.can_undo(),
            AnyGame::FortyEight(g) => g.can_undo(),
            AnyGame::Bakers(g) => g.can_undo(),
            AnyGame::Castle(g) => g.can_undo(),
            AnyGame::Normandy(g) => g.can_undo(),
        }
    }

    pub fn can_redo(&self) -> bool {
        match self {
            AnyGame::Spider(g) => g.can_redo(),
            AnyGame::FreeCell(g) => g.can_redo(),
            AnyGame::Klondike(g) => g.can_redo(),
            AnyGame::Clock(g) => g.can_redo(),
            AnyGame::Simon(g) => g.can_redo(),
            AnyGame::Golf(g) => g.can_redo(),
            AnyGame::Yukon(g) => g.can_redo(),
            AnyGame::Grandfather(g) => g.can_redo(),
            AnyGame::Mod3(g) => g.can_redo(),
            AnyGame::AcesUp(g) => g.can_redo(),
            AnyGame::FortyEight(g) => g.can_redo(),
            AnyGame::Bakers(g) => g.can_redo(),
            AnyGame::Castle(g) => g.can_redo(),
            AnyGame::Normandy(g) => g.can_redo(),
        }
    }

    pub fn undo(&mut self) -> bool {
        match self {
            AnyGame::Spider(g) => g.undo(),
            AnyGame::FreeCell(g) => g.undo(),
            AnyGame::Klondike(g) => g.undo(),
            AnyGame::Clock(g) => g.undo(),
            AnyGame::Simon(g) => g.undo(),
            AnyGame::Golf(g) => g.undo(),
            AnyGame::Yukon(g) => g.undo(),
            AnyGame::Grandfather(g) => g.undo(),
            AnyGame::Mod3(g) => g.undo(),
            AnyGame::AcesUp(g) => g.undo(),
            AnyGame::FortyEight(g) => g.undo(),
            AnyGame::Bakers(g) => g.undo(),
            AnyGame::Castle(g) => g.undo(),
            AnyGame::Normandy(g) => g.undo(),
        }
    }

    pub fn redo(&mut self) -> bool {
        match self {
            AnyGame::Spider(g) => g.redo(),
            AnyGame::FreeCell(g) => g.redo(),
            AnyGame::Klondike(g) => g.redo(),
            AnyGame::Clock(g) => g.redo(),
            AnyGame::Simon(g) => g.redo(),
            AnyGame::Golf(g) => g.redo(),
            AnyGame::Yukon(g) => g.redo(),
            AnyGame::Grandfather(g) => g.redo(),
            AnyGame::Mod3(g) => g.redo(),
            AnyGame::AcesUp(g) => g.redo(),
            AnyGame::FortyEight(g) => g.redo(),
            AnyGame::Bakers(g) => g.redo(),
            AnyGame::Castle(g) => g.redo(),
            AnyGame::Normandy(g) => g.redo(),
        }
    }

    pub fn play(&mut self, m: Move) -> bool {
        match self {
            AnyGame::Spider(g) => g.play(m),
            AnyGame::FreeCell(g) => g.play(m),
            AnyGame::Klondike(g) => g.play(m),
            AnyGame::Clock(g) => g.play(m),
            AnyGame::Simon(g) => g.play(m),
            AnyGame::Golf(g) => g.play(m),
            AnyGame::Yukon(g) => g.play(m),
            AnyGame::Grandfather(g) => g.play(m),
            AnyGame::Mod3(g) => g.play(m),
            AnyGame::AcesUp(g) => g.play(m),
            AnyGame::FortyEight(g) => g.play(m),
            AnyGame::Bakers(g) => g.play(m),
            AnyGame::Castle(g) => g.play(m),
            AnyGame::Normandy(g) => g.play(m),
        }
    }

    /// Spider : reste-t-il des rangées à distribuer ? Klondike : peut-on
    /// piocher (ou retourner la pioche) ? Golf : reste-t-il un talon ?
    /// Grand-père : reste-t-il une redistribution ? Mod3 et les As :
    /// reste-t-il un talon ?
    pub fn can_deal(&self) -> bool {
        match self {
            AnyGame::Spider(g) => !g.stock.is_empty(),
            AnyGame::FreeCell(_) | AnyGame::Clock(_) | AnyGame::Simon(_) | AnyGame::Yukon(_) | AnyGame::Bakers(_) | AnyGame::Castle(_) => false,
            AnyGame::Klondike(g) => g.can_draw(),
            AnyGame::Golf(g) => g.can_draw(),
            AnyGame::Grandfather(g) => g.can_redeal(),
            AnyGame::Mod3(g) => g.can_deal(),
            AnyGame::AcesUp(g) => g.can_deal(),
            AnyGame::FortyEight(g) => g.can_deal(),
            AnyGame::Normandy(g) => g.can_deal(),
        }
    }

    /// Klondike, Golf, Mod3 et les As : la pile du talon (un clic dessus pioche ou
    /// distribue).
    pub fn stock_pile(&self) -> Option<usize> {
        match self {
            AnyGame::Klondike(_) => Some(klondike::STOCK),
            AnyGame::Golf(_) => Some(golf::TALON),
            AnyGame::Mod3(_) => Some(mod3::TALON),
            AnyGame::AcesUp(_) => Some(aces::TALON),
            AnyGame::FortyEight(_) => Some(fortyeight::TALON),
            AnyGame::Normandy(_) => Some(normandy::TALON),
            _ => None,
        }
    }

    /// Les piles où l'on joue (colonnes du Spider ; colonnes, cellules et
    /// fondations du FreeCell). La réserve et les suites sorties du Spider
    /// sont à part.
    pub fn pile_count(&self) -> usize {
        match self {
            AnyGame::Spider(_) => spider::COLUMNS,
            AnyGame::FreeCell(g) => g.piles.len(),
            AnyGame::Klondike(g) => g.piles.len(),
            AnyGame::Clock(g) => g.piles.len(),
            AnyGame::Simon(g) => g.piles.len(),
            AnyGame::Golf(g) => g.piles.len(),
            AnyGame::Yukon(g) => g.piles.len(),
            AnyGame::Grandfather(g) => g.piles.len(),
            AnyGame::Mod3(g) => g.piles.len(),
            AnyGame::AcesUp(g) => g.piles.len(),
            AnyGame::FortyEight(g) => g.piles.len(),
            AnyGame::Bakers(g) => g.piles.len(),
            AnyGame::Castle(g) => g.piles.len(),
            AnyGame::Normandy(g) => g.piles.len(),
        }
    }

    pub fn pile(&self, p: usize) -> &[Card] {
        match self {
            AnyGame::Spider(g) => &g.columns[p],
            AnyGame::FreeCell(g) => &g.piles[p],
            AnyGame::Klondike(g) => &g.piles[p],
            AnyGame::Clock(g) => &g.piles[p],
            AnyGame::Simon(g) => &g.piles[p],
            AnyGame::Golf(g) => &g.piles[p],
            AnyGame::Yukon(g) => &g.piles[p],
            AnyGame::Grandfather(g) => &g.piles[p],
            AnyGame::Mod3(g) => &g.piles[p],
            AnyGame::AcesUp(g) => &g.piles[p],
            AnyGame::FortyEight(g) => &g.piles[p],
            AnyGame::Bakers(g) => &g.piles[p],
            AnyGame::Castle(g) => &g.piles[p],
            AnyGame::Normandy(g) => &g.piles[p],
        }
    }

    pub fn pile_kind(&self, p: usize) -> PileKind {
        match self {
            AnyGame::Spider(_) => PileKind::Tableau,
            AnyGame::FreeCell(g) => match g.rules().kind(p) {
                freecell::PileKind::Tableau => PileKind::Tableau,
                freecell::PileKind::Cell => PileKind::Cell,
                freecell::PileKind::Foundation => PileKind::Foundation,
            },
            AnyGame::Klondike(_) => match klondike::kind(p) {
                klondike::PileKind::Tableau => PileKind::Tableau,
                klondike::PileKind::Waste => PileKind::Waste,
                klondike::PileKind::Stock => PileKind::Stock,
                klondike::PileKind::Foundation => PileKind::Foundation,
            },
            AnyGame::Clock(_) if p < clock::STORES => PileKind::Tableau,
            AnyGame::Clock(_) => PileKind::Foundation,
            AnyGame::Simon(_) if p < simon::STORES => PileKind::Tableau,
            AnyGame::Simon(_) => PileKind::Foundation,
            AnyGame::Golf(_) => match p {
                0..golf::STORES => PileKind::Tableau,
                golf::WASTE => PileKind::Waste,
                _ => PileKind::Stock,
            },
            AnyGame::Yukon(_) if p < yukon::STORES => PileKind::Tableau,
            AnyGame::Yukon(_) => PileKind::Foundation,
            AnyGame::Grandfather(_) if p < grandf::STORES => PileKind::Tableau,
            AnyGame::Grandfather(_) => PileKind::Foundation,
            AnyGame::Mod3(_) if (mod3::RESERVE..mod3::ACES).contains(&p) => PileKind::Tableau,
            AnyGame::Mod3(_) if p == mod3::TALON => PileKind::Stock,
            AnyGame::Mod3(_) => PileKind::Foundation,
            AnyGame::AcesUp(_) => match p {
                0..aces::STORES => PileKind::Tableau,
                aces::AWAY => PileKind::Foundation,
                _ => PileKind::Stock,
            },
            AnyGame::Normandy(_) => match p {
                0..normandy::STORES => PileKind::Tableau,
                normandy::TALON => PileKind::Stock,
                _ => PileKind::Foundation,
            },
            AnyGame::Bakers(_) if p < bakers::STORES => PileKind::Tableau,
            AnyGame::Bakers(_) => PileKind::Foundation,
            AnyGame::Castle(g) => match g.rules().kind(p) {
                freecell::PileKind::Tableau => PileKind::Tableau,
                freecell::PileKind::Cell => PileKind::Cell,
                freecell::PileKind::Foundation => PileKind::Foundation,
            },
            AnyGame::FortyEight(_) => match p {
                0..fortyeight::STORES => PileKind::Tableau,
                fortyeight::WASTE => PileKind::Waste,
                fortyeight::TALON => PileKind::Stock,
                _ => PileKind::Foundation,
            },
        }
    }

    pub fn can_pick(&self, p: usize, index: usize) -> bool {
        match self {
            AnyGame::Spider(g) => g.can_pick(p, index),
            AnyGame::FreeCell(g) => g.can_pick(p, index),
            AnyGame::Klondike(g) => g.can_pick(p, index),
            AnyGame::Clock(g) => g.can_pick(p, index),
            AnyGame::Simon(g) => g.can_pick(p, index),
            AnyGame::Golf(g) => g.can_pick(p, index),
            AnyGame::Yukon(g) => g.can_pick(p, index),
            AnyGame::Grandfather(g) => g.can_pick(p, index),
            AnyGame::Mod3(g) => g.can_pick(p, index),
            AnyGame::AcesUp(g) => g.can_pick(p, index),
            AnyGame::FortyEight(g) => g.can_pick(p, index),
            AnyGame::Bakers(g) => g.can_pick(p, index),
            AnyGame::Castle(g) => g.can_pick(p, index),
            AnyGame::Normandy(g) => g.can_pick(p, index),
        }
    }

    pub fn can_move(&self, from: usize, index: usize, to: usize) -> bool {
        let m = Move::Cards { from, index, to };
        match self {
            AnyGame::Spider(g) => g.is_legal(m),
            AnyGame::FreeCell(g) => g.is_legal(m),
            AnyGame::Klondike(g) => g.is_legal(m),
            AnyGame::Clock(g) => g.is_legal(m),
            AnyGame::Simon(g) => g.is_legal(m),
            AnyGame::Golf(g) => g.is_legal(m),
            AnyGame::Yukon(g) => g.is_legal(m),
            AnyGame::Grandfather(g) => g.is_legal(m),
            AnyGame::Mod3(g) => g.is_legal(m),
            AnyGame::AcesUp(g) => g.is_legal(m),
            AnyGame::FortyEight(g) => g.is_legal(m),
            AnyGame::Bakers(g) => g.is_legal(m),
            AnyGame::Castle(g) => g.is_legal(m),
            AnyGame::Normandy(g) => g.is_legal(m),
        }
    }

    /// Double-clic sur une carte.
    pub fn auto_move(&self, p: usize, index: usize) -> Option<Move> {
        match self {
            AnyGame::Spider(g) => g.auto_move(p, index),
            AnyGame::FreeCell(g) => g.auto_move(p, index),
            AnyGame::Klondike(g) => g.auto_move(p, index),
            AnyGame::Clock(g) => g.auto_move(p, index),
            AnyGame::Simon(g) => g.auto_move(p, index),
            AnyGame::Golf(g) => g.auto_move(p, index),
            AnyGame::Yukon(g) => g.auto_move(p, index),
            AnyGame::Grandfather(g) => g.auto_move(p, index),
            AnyGame::Mod3(g) => g.auto_move(p, index),
            AnyGame::AcesUp(g) => g.auto_move(p, index),
            AnyGame::FortyEight(g) => g.auto_move(p, index),
            AnyGame::Bakers(g) => g.auto_move(p, index),
            AnyGame::Castle(g) => g.auto_move(p, index),
            AnyGame::Normandy(g) => g.auto_move(p, index),
        }
    }

    pub fn sensible_moves(&self) -> Vec<Move> {
        match self {
            AnyGame::Spider(g) => g.sensible_moves(),
            AnyGame::FreeCell(g) => g.sensible_moves(),
            AnyGame::Klondike(g) => klondike_solver::useful_moves(g),
            AnyGame::Clock(g) => clock_solver::useful_moves(g),
            AnyGame::Simon(g) => g.sensible_moves(),
            AnyGame::Golf(g) => g.sensible_moves(),
            AnyGame::Yukon(g) => yukon_solver::useful_moves(g),
            AnyGame::Grandfather(g) => grandf_solver::useful_moves(g),
            AnyGame::Mod3(g) => mod3_solver::useful_moves(g),
            AnyGame::AcesUp(g) => g.sensible_moves(),
            AnyGame::FortyEight(g) => fortyeight_solver::useful_moves(g),
            AnyGame::Bakers(g) => g.sensible_moves(),
            AnyGame::Castle(g) => g.sensible_moves(),
            AnyGame::Normandy(g) => normandy_solver::useful_moves(g),
        }
    }

    /// FreeCell : le prochain dépôt automatique vers les fondations.
    pub fn autodrop(&self) -> Option<Move> {
        match self {
            AnyGame::Spider(_) => None,
            AnyGame::FreeCell(g) => g.autodrop(),
            AnyGame::Klondike(g) => g.autodrop(),
            // `Clock::drop` : pas de dépôt automatique.
            AnyGame::Clock(_) => None,
            AnyGame::Simon(g) => g.autodrop(),
            AnyGame::Golf(g) => g.autodrop(),
            AnyGame::Yukon(g) => g.autodrop(),
            AnyGame::Grandfather(g) => g.autodrop(),
            AnyGame::Mod3(g) => g.autodrop(),
            // `Idiot::drop` : pas de dépôt automatique.
            AnyGame::AcesUp(_) => None,
            AnyGame::FortyEight(g) => g.autodrop(),
            AnyGame::Bakers(g) => g.autodrop(),
            AnyGame::Castle(g) => g.autodrop(),
            AnyGame::Normandy(g) => g.autodrop(),
        }
    }

    /// Klondike : piocher aussitôt après un dépôt automatique qui a vidé la
    /// pioche (`Klondike::drop`).
    pub fn draw_after_drop(&self, m: Move) -> bool {
        matches!(self, AnyGame::Klondike(g) if g.draw_after_drop(m))
    }

    pub fn uses_autodrop(&self) -> bool {
        matches!(self, AnyGame::FreeCell(_) | AnyGame::Klondike(_) | AnyGame::Simon(_) | AnyGame::Golf(_) | AnyGame::Yukon(_) | AnyGame::Grandfather(_) | AnyGame::Mod3(_) | AnyGame::FortyEight(_) | AnyGame::Bakers(_) | AnyGame::Castle(_) | AnyGame::Normandy(_))
    }

    /// Clé de la position, pour garder les résultats du solveur.
    pub fn position_key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(180);
        match self {
            AnyGame::Spider(g) => {
                key.push(1);
                for col in &g.columns {
                    for c in col {
                        key.push(c.rank | ((c.suit as u8) << 4) | if c.face_up { 64 } else { 0 });
                    }
                    key.push(255);
                }
                key.push(g.stock.len() as u8);
                key.push(g.runs.len() as u8);
            }
            AnyGame::FreeCell(g) => {
                key.push(2);
                let o = g.options;
                key.extend([o.empty_fill as u8, o.build as u8, o.cells, o.stacks, o.decks]);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::Klondike(g) => {
                key.push(3);
                key.push(g.draw);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::Simon(g) => {
                key.push(5);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::Mod3(g) => {
                key.push(9);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::Grandfather(g) => {
                key.push(8);
                key.push(g.deals);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::Yukon(g) => {
                key.push(7);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::Golf(g) => {
                key.push(6);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::Normandy(g) => {
                key.push(14);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::Castle(g) => {
                key.push(13);
                let o = g.castle;
                key.extend([o.empty_fill as u8, o.build as u8, o.reserves, o.stacks, o.faces as u8, o.prefill as u8]);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::Bakers(g) => {
                key.push(12);
                let o = g.bakers;
                key.extend([o.empty_fill as u8, o.build as u8, o.face_down as u8]);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code() | if c.face_up { 64 } else { 0 }));
                    key.push(255);
                }
            }
            AnyGame::FortyEight(g) => {
                key.push(11);
                key.push(g.lastdeal as u8);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::AcesUp(g) => {
                key.push(10);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
            AnyGame::Clock(g) => {
                key.push(4);
                for p in &g.piles {
                    key.extend(p.iter().map(|c| c.code()));
                    key.push(255);
                }
            }
        }
        key
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &AnyGame, stop: &AtomicBool) -> Outcome {
    match game {
        AnyGame::Spider(g) => match spider_solver::solve(g, stop) {
            spider_solver::Outcome::Winnable(path) => {
                Outcome::Winnable(path.into_iter().filter_map(spider_solver::SolverMove::to_game_move).collect())
            }
            spider_solver::Outcome::NotWinnable => Outcome::NotWinnable,
            spider_solver::Outcome::Unknown => Outcome::Unknown,
            spider_solver::Outcome::Aborted => Outcome::Aborted,
        },
        AnyGame::FreeCell(g) => freecell_solver::solve(g, stop),
        AnyGame::Klondike(g) => klondike_solver::solve(g, stop),
        AnyGame::Clock(g) => clock_solver::solve(g, stop),
        AnyGame::Simon(g) => simon_solver::solve(g, stop),
        AnyGame::Golf(g) => golf_solver::solve(g, stop),
        AnyGame::Yukon(g) => yukon_solver::solve(g, stop),
        AnyGame::Grandfather(g) => grandf_solver::solve(g, stop),
        AnyGame::Mod3(g) => mod3_solver::solve(g, stop),
        AnyGame::AcesUp(g) => aces_solver::solve(g, stop),
        AnyGame::FortyEight(g) => fortyeight_solver::solve(g, stop),
        AnyGame::Bakers(g) => bakers::solve(g, stop),
        AnyGame::Castle(g) => castle::solve(g, stop),
        AnyGame::Normandy(g) => normandy_solver::solve(g, stop),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque jeu se relit comme lui-même dans la partie enregistrée
    /// (`AnyGame` est `untagged`).
    #[test]
    fn saved_games_keep_their_kind() {
        let games = [
            AnyGame::Spider(spider::Game::new(2, 3, false)),
            AnyGame::FreeCell(freecell::Game::new(freecell::Options::default(), 3)),
            AnyGame::Klondike(klondike::Game::new(3, 3)),
            AnyGame::Clock(clock::Game::new(3)),
            AnyGame::Simon(simon::Game::new(3)),
            AnyGame::Golf(golf::Game::new(3)),
            AnyGame::Yukon(yukon::Game::new(3)),
            AnyGame::Grandfather(grandf::Game::new(3)),
            AnyGame::Mod3(mod3::Game::new(3)),
            AnyGame::AcesUp(aces::Game::new(3)),
            AnyGame::FortyEight(fortyeight::Game::new(3)),
            AnyGame::Bakers(bakers::Game::new(bakers::PRESETS[2].1, 3)),
            AnyGame::Castle(castle::Game::new(castle::PRESETS[4].1, 3)),
            AnyGame::Normandy(normandy::Game::new(3)),
        ];
        for g in games {
            let json = serde_json::to_string(&g).unwrap();
            let back: AnyGame = serde_json::from_str(&json).unwrap();
            assert_eq!(back.kind(), g.kind());
            assert_eq!(back.position_key(), g.position_key());
        }
    }
}
