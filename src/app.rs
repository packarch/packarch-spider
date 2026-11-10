//! Interface du jeu (egui / eframe).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui::{
    self, Align2, Color32, ColorImage, Context, FontId, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Stroke, TextureHandle,
    TextureOptions, Vec2,
};

use crate::assets::{self, AssetInfo, Svg};
use crate::board::{self, AnyGame, GameKind, Move, Outcome, PileKind, Variant};
use crate::cards::{self, Card};
use crate::freecell::{self, Build, EmptyFill};
use crate::spider::{self, ROWS};
use crate::storage::{self, SavedState, Settings, Stats};
use crate::{aces, bakers, castle, clock, normandy, fortyeight, golf, grandf, klondike, mod3, simon, yukon};

const APP_NAME: &str = "Packarch Spider";
/// Écart horizontal entre colonnes du Spider, en largeurs de carte (comme KPatience).
const DIST_X: f32 = 1.12;
/// Marge et espacement autour des piles, en tailles de carte (KPatience).
const LAYOUT_MARGIN: f32 = 0.15;
const LAYOUT_SPACING: f32 = 0.15;
/// Écart vertical entre deux cartes visibles d'une colonne, en hauteurs de
/// carte, et coefficient pour les cartes face cachée (KPatience).
const SPREAD: f32 = 0.33;
const FACE_DOWN_SPREAD: f32 = 0.6;
const DEMO_DELAY: f64 = 0.35;
const SOLVER_DELAY: f64 = 0.25;

// =================================================================== solveur

#[derive(Clone, Copy, PartialEq, Eq)]
enum SolverStatus {
    Off,
    Calculating,
    Winnable,
    NotWinnable,
    Unknown,
}

/// L'outil de résolution tourne dans un fil à part et recommence après
/// chaque coup (comme dans KPatience). Les résultats sont gardés par
/// position, pour que « Annuler » soit instantané.
struct SolverManager {
    status: SolverStatus,
    /// Chemin gagnant depuis la position actuelle.
    path: Option<Vec<Move>>,
    ever_winnable: bool,
    start_at: Option<f64>,
    job: Option<(Arc<AtomicBool>, Receiver<Outcome>, Vec<u8>)>,
    cache: HashMap<Vec<u8>, Outcome>,
}

impl SolverManager {
    fn new() -> SolverManager {
        SolverManager { status: SolverStatus::Off, path: None, ever_winnable: false, start_at: None, job: None, cache: HashMap::new() }
    }

    fn stop_job(&mut self) {
        if let Some((stop, _, _)) = self.job.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }

    fn apply(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Winnable(path) => {
                self.status = SolverStatus::Winnable;
                self.path = Some(path.clone());
                self.ever_winnable = true;
            }
            Outcome::NotWinnable => {
                self.status = SolverStatus::NotWinnable;
                self.path = None;
            }
            Outcome::Unknown | Outcome::Aborted => {
                self.status = SolverStatus::Unknown;
                self.path = None;
            }
        }
    }

    /// La position a changé. `played` : le coup qui vient d'être joué, s'il
    /// y en a un ; s'il suit le chemin gagnant, inutile de recalculer.
    fn position_changed(&mut self, game: &AnyGame, played: Option<Move>, enabled: bool, now: f64) {
        self.stop_job();
        if !enabled {
            self.status = SolverStatus::Off;
            self.path = None;
            self.start_at = None;
            return;
        }
        if let (Some(m), Some(path)) = (played, self.path.as_mut()) {
            if path.first() == Some(&m) {
                path.remove(0);
                self.status = SolverStatus::Winnable;
                self.cache.insert(game.position_key(), Outcome::Winnable(path.clone()));
                self.start_at = None;
                return;
            }
        }
        self.path = None;
        match self.cache.get(&game.position_key()).cloned() {
            Some(outcome) => {
                self.apply(&outcome);
                self.start_at = None;
            }
            None => {
                self.status = SolverStatus::Calculating;
                self.start_at = Some(now + SOLVER_DELAY);
            }
        }
    }

    /// Démarre le calcul quand c'est l'heure, récupère son résultat.
    /// Renvoie `true` si un résultat vient d'arriver.
    fn poll(&mut self, game: &AnyGame, now: f64) -> bool {
        if let Some(t) = self.start_at {
            if now >= t {
                self.start_at = None;
                let stop = Arc::new(AtomicBool::new(false));
                let (tx, rx) = mpsc::channel();
                let g = game.clone();
                let s = stop.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(board::solve(&g, &s));
                });
                self.job = Some((stop, rx, game.position_key()));
            }
        }
        let mut finished = None;
        if let Some((_, rx, key)) = &self.job {
            match rx.try_recv() {
                Ok(outcome) => finished = Some((outcome, key.clone())),
                Err(TryRecvError::Disconnected) => finished = Some((Outcome::Unknown, key.clone())),
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some((outcome, key)) = finished {
            self.job = None;
            if outcome != Outcome::Aborted {
                self.cache.insert(key, outcome.clone());
            }
            self.apply(&outcome);
            return true;
        }
        false
    }

    fn calculating(&self) -> bool {
        self.status == SolverStatus::Calculating
    }

    fn message(&self) -> &'static str {
        match self.status {
            SolverStatus::Off => "",
            SolverStatus::Calculating => "Outil de résolution : calcul en cours…",
            SolverStatus::Winnable => "Outil de résolution : vous pouvez gagner cette partie.",
            SolverStatus::NotWinnable if self.ever_winnable => "Outil de résolution : vous ne pouvez plus gagner cette partie.",
            SolverStatus::NotWinnable => "Outil de résolution : cette partie ne peut pas être gagnée.",
            SolverStatus::Unknown => "Outil de résolution : impossible de déterminer si la partie peut être gagnée.",
        }
    }

    /// Le prochain coup du chemin gagnant.
    fn next_move(&self) -> Option<Move> {
        self.path.as_ref()?.first().copied()
    }
}

// =================================================================== textures

/// Images rendues depuis un SVG en arrière-plan, à une taille donnée.
struct Rendered {
    size: (u32, u32),
    textures: HashMap<String, TextureHandle>,
    pending: Option<((u32, u32), Receiver<Vec<(String, ColorImage)>>)>,
    /// Change quand le SVG change : les images en cours sont jetées.
    source: u64,
}

impl Rendered {
    fn new() -> Rendered {
        Rendered { size: (0, 0), textures: HashMap::new(), pending: None, source: 0 }
    }

    fn reset(&mut self) {
        self.size = (0, 0);
        self.textures.clear();
        self.pending = None;
        self.source += 1;
    }

    /// Demande les éléments `names` à la taille `size` ; renvoie vrai si un
    /// rendu est en cours (il faut alors redessiner plus tard).
    fn request(&mut self, ctx: &Context, svg: &Arc<Svg>, names: Vec<(String, (u32, u32))>, size: (u32, u32)) -> bool {
        if let Some((psize, rx)) = &self.pending {
            match rx.try_recv() {
                Ok(images) => {
                    for (name, image) in images {
                        let handle = ctx.load_texture(format!("{name}-{}", self.source), image, TextureOptions::LINEAR);
                        self.textures.insert(name, handle);
                    }
                    self.size = *psize;
                    self.pending = None;
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if size != self.size && size.0 > 0 && size.1 > 0 {
            let (tx, rx) = mpsc::channel();
            let svg = svg.clone();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                let images: Vec<(String, ColorImage)> =
                    names.into_iter().filter_map(|(name, (w, h))| svg.render(&name, w, h).map(|img| (name, img))).collect();
                let _ = tx.send(images);
                ctx2.request_repaint();
            });
            self.pending = Some((size, rx));
            return true;
        }
        false
    }
}

// =================================================================== disposition

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Place {
    /// Une carte d'une pile où l'on joue.
    Pile(usize, usize),
    /// Spider : un tas de la réserve.
    Stock(usize),
    /// Spider : une suite sortie.
    Run(usize),
}

/// Une pile à l'écran.
struct PileGeom {
    /// Coin de la première carte.
    origin: Pos2,
    /// Cartes étalées vers le bas (colonnes), empilées, ou les dernières
    /// étalées vers la droite (pioche du Klondike).
    fan: Fan,
    /// Écarts (face cachée, face visible) entre cartes étalées.
    gaps: (f32, f32),
    /// Zone où lâcher des cartes.
    zone: Rect,
}

#[derive(Clone, Copy, PartialEq)]
enum Fan {
    None,
    Down,
    /// Les `n` dernières cartes, décalées de `gap` vers la droite.
    Right {
        n: usize,
        gap: f32,
    },
}

/// Spider : la réserve en bas à droite, les suites sorties en bas à gauche.
struct SpiderGeom {
    left: f32,
    right: f32,
    bottom: f32,
}

struct Layout {
    card: Vec2,
    piles: Vec<PileGeom>,
    /// D'où partent les cartes pour l'animation de la distribution.
    deal_origin: Pos2,
    spider: Option<SpiderGeom>,
}

/// Les écarts d'une colonne étalée, resserrés pour tenir en `avail` de haut.
fn fan_gaps(cards: &[Card], h: f32, avail: f32) -> (f32, f32) {
    let (mut down, mut up) = (SPREAD * FACE_DOWN_SPREAD * h, SPREAD * h);
    if cards.len() > 1 {
        let downs = cards[..cards.len() - 1].iter().filter(|c| !c.face_up).count() as f32;
        let ups = (cards.len() - 1) as f32 - downs;
        let needed = downs * down + ups * up;
        if needed > avail {
            let k = avail / needed;
            down *= k;
            up *= k;
        }
    }
    (down, up)
}

impl Layout {
    fn new(rect: Rect, ratio: f32, game: &AnyGame) -> Layout {
        match game {
            AnyGame::Spider(g) => Layout::spider(rect, ratio, g),
            AnyGame::FreeCell(g) => Layout::freecell(rect, ratio, g),
            AnyGame::Klondike(g) => Layout::klondike(rect, ratio, g),
            AnyGame::Clock(g) => Layout::clock(rect, ratio, g),
            AnyGame::Simon(g) => Layout::simon(rect, ratio, g),
            AnyGame::Golf(g) => Layout::golf(rect, ratio, g),
            AnyGame::Yukon(g) => Layout::yukon(rect, ratio, g),
            AnyGame::Grandfather(g) => Layout::grandfather(rect, ratio, g),
            AnyGame::Mod3(g) => Layout::mod3(rect, ratio, g),
            AnyGame::AcesUp(g) => Layout::aces(rect, ratio, g),
            AnyGame::FortyEight(g) => Layout::fortyeight(rect, ratio, g),
            AnyGame::Bakers(g) => Layout::bakers(rect, ratio, g),
            AnyGame::Castle(g) => Layout::castle(rect, ratio, g),
            AnyGame::Normandy(g) => Layout::normandy(rect, ratio, g),
        }
    }

    /// Disposition de `Mod3::initialize` : les 3 rangées de fondations
    /// (cartes à peine décalées), la réserve en bas, la pile des As et le
    /// talon dans la colonne de droite. Positions en largeurs (x) et hauteurs
    /// (y) de carte.
    fn mod3(rect: Rect, ratio: f32, game: &mod3::Game) -> Layout {
        const DIST_X: f32 = 1.114;
        const DIST_Y: f32 = 1.31;
        const BOTTOM_ROW_Y: f32 = 3.0 * DIST_Y + 0.2;
        const RIGHT_X: f32 = 8.0 * DIST_X + 0.8;
        const TIGHT: f32 = 0.08;
        const RESERVE_PADDING: f32 = 0.8;
        const ACES_PADDING: f32 = 2.5;
        let mut units = [(0.0, 0.0); mod3::PILES];
        for (p, u) in units.iter_mut().enumerate().take(mod3::ACES) {
            let (r, c) = (p / mod3::COLUMNS, p % mod3::COLUMNS);
            *u = (DIST_X * c as f32, if r < 3 { DIST_Y * r as f32 } else { BOTTOM_ROW_Y });
        }
        units[mod3::ACES] = (RIGHT_X, 0.5);
        units[mod3::TALON] = (RIGHT_X, BOTTOM_ROW_Y);
        let width_units = RIGHT_X + 1.0;
        let height_units = BOTTOM_ROW_Y + 1.0 + RESERVE_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(p, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                let card = Rect::from_min_size(origin, Vec2::new(w, h));
                match p {
                    0..mod3::RESERVE => PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: (TIGHT * h, TIGHT * h),
                        zone: Rect::from_min_size(origin, Vec2::new(w, (1.0 + 3.0 * TIGHT) * h)),
                    },
                    mod3::RESERVE..mod3::ACES => {
                        let avail = (bottom - origin.y - h).max(h * 0.3);
                        PileGeom {
                            origin,
                            fan: Fan::Down,
                            gaps: fan_gaps(&game.piles[p], h, avail),
                            zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                        }
                    }
                    mod3::ACES => PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[p], h, ACES_PADDING * h),
                        zone: Rect::from_min_size(origin, Vec2::new(w, (1.0 + ACES_PADDING) * h)),
                    },
                    _ => PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: card },
                }
            })
            .collect();
        let deal_origin = piles[mod3::TALON].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Grandf::initialize` : les 4 fondations en haut,
    /// décalées vers la droite, les 7 colonnes en dessous. Positions en
    /// largeurs (x) et hauteurs (y) de carte.
    fn grandfather(rect: Rect, ratio: f32, game: &grandf::Game) -> Layout {
        const DIST_X: f32 = 1.4;
        const BOTTOM_PADDING: f32 = 2.5;
        let mut units = [(0.0, 0.0); grandf::PILES];
        for (i, u) in units.iter_mut().enumerate().take(grandf::STORES) {
            *u = (DIST_X * i as f32, 1.2);
        }
        for i in 0..4 {
            units[grandf::FOUNDATION + i] = (1.5 * DIST_X + i as f32 * DIST_X, 0.0);
        }
        let width_units = 6.0 * DIST_X + 1.0;
        let height_units = 1.2 + 1.0 + BOTTOM_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < grandf::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // KPatience distribue depuis le milieu de la rangée des colonnes.
        let deal_origin = piles[3].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Yukon::initialize` : les 7 colonnes à gauche, les 4
    /// fondations en colonne à droite. Positions en largeurs (x) et hauteurs
    /// (y) de carte.
    fn yukon(rect: Rect, ratio: f32, game: &yukon::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const DIST_Y: f32 = 1.11;
        const BOTTOM_PADDING: f32 = 3.0 * DIST_Y;
        let mut units = [(0.0, 0.0); yukon::PILES];
        for (i, u) in units.iter_mut().enumerate().take(yukon::STORES) {
            *u = (DIST_X * i as f32, 0.0);
        }
        for i in 0..4 {
            units[yukon::FOUNDATION + i] = (0.11 + 7.0 * DIST_X, DIST_Y * i as f32);
        }
        let width_units = 0.11 + 7.0 * DIST_X + 1.0;
        let height_units = 1.0 + BOTTOM_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < yukon::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // KPatience distribue depuis le bas de l'écran.
        let deal_origin = Pos2::new(piles[0].origin.x, rect.bottom());
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Normandy::initialize` : les 8 colonnes à gauche, les
    /// 8 fondations en deux colonnes à droite, le talon en dessous. Positions
    /// en largeurs (x) et hauteurs (y) de carte.
    fn normandy(rect: Rect, ratio: f32, game: &normandy::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const DIST_Y: f32 = 1.11;
        let mut units = [(0.0, 0.0); normandy::PILES];
        for (i, u) in units.iter_mut().enumerate().take(normandy::STORES) {
            *u = (DIST_X * i as f32, 0.0);
        }
        for i in 0..8 {
            units[normandy::FOUNDATION + i] = (DIST_X * (8 + i / 4) as f32 + 0.4, (i % 4) as f32 * DIST_Y);
        }
        units[normandy::TALON] = (8.5 * DIST_X + 0.4, 4.0 * DIST_Y);
        let width_units = 9.0 * DIST_X + 0.4 + 1.0;
        let height_units = 4.0 * DIST_Y + 1.0;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < normandy::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        let deal_origin = piles[normandy::TALON].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Castle::initialize` : les réserves en haut, les
    /// fondations en colonne au milieu, les colonnes de part et d'autre, deux
    /// par rangée ; en disposition classique, celles de gauche s'étalent vers
    /// la gauche, sinon toutes vers la droite. Positions en largeurs (x) et
    /// hauteurs (y) de carte.
    fn castle(rect: Rect, ratio: f32, game: &castle::Game) -> Layout {
        const ROW: f32 = 1.125;
        const SPREAD_X: f32 = 0.21;
        const LEFT_ROOM: f32 = 3.0;
        const RIGHT_ROOM: f32 = 2.0;
        let o = game.castle;
        let r = game.rules();
        let (stacks, cells) = (r.stacks(), r.cells());
        let offset_y = if cells > 0 { 1.3 } else { 0.0 };
        let rows = stacks.div_ceil(2).max(4);
        // Bord gauche de la scène, en unités : -1 en classique (la colonne de
        // gauche à x = 2 s'étale de 3 cartes), 0 en moderne.
        let left = if o.modern { 0.0 } else { -1.0 };
        let width_units = 4.5 + 1.0 + RIGHT_ROOM - left;
        let height_units = offset_y + ROW * (rows - 1) as f32 + 1.0;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0 - left * w;
        let y0 = rect.top() + ((rect.height() - height_units * h) / 2.0).max(LAYOUT_MARGIN * h);
        let at = |x: f32, y: f32| Pos2::new(x0 + x * w, y0 + y * h);
        let piles: Vec<PileGeom> = (0..r.piles())
            .map(|i| match r.kind(i) {
                freecell::PileKind::Tableau => {
                    let row = (i / 2) as f32;
                    let x = if o.modern { 4.5 * (i % 2) as f32 } else { 2.5 * (i % 2) as f32 + 2.0 };
                    let origin = at(x, offset_y + ROW * row);
                    let leftward = !o.modern && i % 2 == 0;
                    let room = if leftward { LEFT_ROOM } else { RIGHT_ROOM } * w;
                    let n = game.piles[i].len();
                    let gap = if n > 1 { (SPREAD_X * w).min(room / (n - 1) as f32) } else { SPREAD_X * w };
                    let zone = if leftward {
                        Rect::from_min_max(Pos2::new(origin.x - room, origin.y), Pos2::new(origin.x + w, origin.y + h))
                    } else {
                        Rect::from_min_max(origin, Pos2::new(origin.x + w + room, origin.y + h))
                    };
                    PileGeom { origin, fan: Fan::Right { n: usize::MAX, gap: if leftward { -gap } else { gap } }, gaps: (0.0, 0.0), zone }
                }
                freecell::PileKind::Cell => {
                    let k = (i - r.first_cell()) as f32;
                    let origin = at(3.25 - ROW * (cells - 1) as f32 / 2.0 + ROW * k, 0.0);
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
                freecell::PileKind::Foundation => {
                    let k = (i - r.first_foundation()) as f32;
                    let origin = at(3.25, offset_y + ROW * k);
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // KPatience distribue depuis la première colonne.
        let deal_origin = piles[0].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `BakersDozen::initialize` : les 4 fondations en haut
    /// (une place sur deux), les 13 colonnes en dessous. Positions en
    /// largeurs (x) et hauteurs (y) de carte.
    fn bakers(rect: Rect, ratio: f32, game: &bakers::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const STORES_Y: f32 = 1.2;
        const BOTTOM_PADDING: f32 = 2.0;
        let mut units = [(0.0, 0.0); bakers::PILES];
        for (i, u) in units.iter_mut().enumerate().take(bakers::STORES) {
            *u = (DIST_X * i as f32, STORES_Y);
        }
        for i in 0..4 {
            units[bakers::FOUNDATION + i] = ((i * 2 + 3) as f32 * DIST_X, 0.0);
        }
        let width_units = 12.0 * DIST_X + 1.0;
        let height_units = STORES_Y + 1.0 + BOTTOM_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < bakers::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // KPatience distribue depuis une hauteur de carte au-dessus.
        let deal_origin = Pos2::new(x0, y0 - h);
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Fortyeight::initialize` : les 8 fondations en haut,
    /// les 8 colonnes en dessous ; le talon en bas à droite (ancré au bas de
    /// la scène dans KPatience), le rebut à sa gauche, étalé vers la gauche.
    /// Positions en largeurs (x) et hauteurs (y) de carte.
    fn fortyeight(rect: Rect, ratio: f32, game: &fortyeight::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const STORES_Y: f32 = 1.1;
        const BOTTOM_PADDING: f32 = 1.75;
        const WASTE_SPREAD: f32 = 0.21;
        let width_units = 7.0 * DIST_X + 1.0;
        let height_units = STORES_Y + 1.0 + BOTTOM_PADDING + LAYOUT_SPACING + 1.0;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        // La rangée du talon au bas de la scène.
        let row_y = (rect.bottom() - LAYOUT_MARGIN * h - h).max(y0 + (height_units - 1.0) * h);
        let waste = game.piles[fortyeight::WASTE].len();
        let room = 6.0 * DIST_X * w;
        let gap = if waste > 1 { (WASTE_SPREAD * w).min(room / (waste - 1) as f32) } else { WASTE_SPREAD * w };
        let piles: Vec<PileGeom> = (0..fortyeight::PILES)
            .map(|i| {
                let card = |x: f32, y: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
                match i {
                    0..fortyeight::STORES => {
                        let origin = Pos2::new(x0 + DIST_X * i as f32 * w, y0 + STORES_Y * h);
                        let avail = (row_y - LAYOUT_SPACING * h - origin.y - h).max(h * 0.5);
                        PileGeom {
                            origin,
                            fan: Fan::Down,
                            gaps: fan_gaps(&game.piles[i], h, avail),
                            zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, row_y - LAYOUT_SPACING * h)),
                        }
                    }
                    fortyeight::WASTE => {
                        let origin = Pos2::new(x0 + 6.0 * DIST_X * w, row_y);
                        PileGeom {
                            origin,
                            fan: Fan::Right { n: usize::MAX, gap: -gap },
                            gaps: (0.0, 0.0),
                            zone: Rect::from_min_max(Pos2::new(x0, row_y), Pos2::new(origin.x + w, row_y + h)),
                        }
                    }
                    fortyeight::TALON => {
                        let origin = Pos2::new(x0 + 7.0 * DIST_X * w, row_y);
                        PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: card(origin.x, origin.y) }
                    }
                    _ => {
                        let origin = Pos2::new(x0 + DIST_X * (i - fortyeight::FOUNDATION) as f32 * w, y0);
                        PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: card(origin.x, origin.y) }
                    }
                }
            })
            .collect();
        // KPatience distribue les colonnes depuis 2 hauteurs de carte au-dessus.
        let deal_origin = Pos2::new(piles[0].origin.x, piles[0].origin.y - 2.0 * h);
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Idiot::initialize` : le talon à gauche, les 4
    /// colonnes (étalées vers le bas), les cartes écartées à droite.
    /// Positions en largeurs (x) et hauteurs (y) de carte.
    fn aces(rect: Rect, ratio: f32, game: &aces::Game) -> Layout {
        const DIST_X: f32 = 1.1;
        const BOTTOM_PADDING: f32 = 2.0;
        let mut units = [(0.0, 0.0); aces::PILES];
        for (i, u) in units.iter_mut().enumerate().take(aces::STORES) {
            *u = (1.5 + DIST_X * i as f32, 0.0);
        }
        units[aces::TALON] = (0.0, 0.0);
        units[aces::AWAY] = (1.9 + DIST_X * 4.0, 0.0);
        let width_units = 1.9 + DIST_X * 4.0 + 1.0;
        let height_units = 1.0 + BOTTOM_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < aces::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        let deal_origin = piles[aces::TALON].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Golf::initialize` : les 7 colonnes en haut ; le talon
    /// et le rebut en bas (ancrés au bas de la scène dans KPatience), le
    /// rebut étalé vers la droite. Positions en largeurs (x) et hauteurs (y)
    /// de carte.
    fn golf(rect: Rect, ratio: f32, game: &golf::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const BOTTOM_PADDING: f32 = 1.3;
        const WASTE_SPREAD: f32 = 0.12;
        let row_y = 1.0 + BOTTOM_PADDING + LAYOUT_SPACING;
        let mut units = [(0.0, 0.0); golf::PILES];
        for (i, u) in units.iter_mut().enumerate().take(golf::STORES) {
            *u = (DIST_X * i as f32, 0.0);
        }
        units[golf::TALON] = (0.0, row_y);
        units[golf::WASTE] = (1.1, row_y);
        let width_units = 6.0 * DIST_X + 1.0;
        let height_units = row_y + 1.0;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        // Centré en hauteur : la scène est peu haute.
        let y0 = rect.top() + ((rect.height() - height_units * h) / 2.0).max(LAYOUT_MARGIN * h);
        // Le rebut grandit vers la droite, resserré pour tenir jusqu'au bord.
        let waste = game.piles[golf::WASTE].len();
        let room = (width_units - 1.1 - 1.0) * w;
        let gap = if waste > 1 { (WASTE_SPREAD * w).min(room / (waste - 1) as f32) } else { WASTE_SPREAD * w };
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                let card = Rect::from_min_size(origin, Vec2::new(w, h));
                match i {
                    0..golf::STORES => PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, BOTTOM_PADDING * h),
                        zone: Rect::from_min_size(origin, Vec2::new(w, (1.0 + BOTTOM_PADDING) * h)),
                    },
                    golf::WASTE => PileGeom {
                        origin,
                        fan: Fan::Right { n: usize::MAX, gap },
                        gaps: (0.0, 0.0),
                        zone: Rect::from_min_size(origin, Vec2::new(w + room, h)),
                    },
                    _ => PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: card },
                }
            })
            .collect();
        let deal_origin = piles[golf::TALON].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Simon::initialize` : les 4 fondations en haut, les 10
    /// colonnes en dessous. Positions en largeurs (x) et hauteurs (y) de carte.
    fn simon(rect: Rect, ratio: f32, game: &simon::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const BOTTOM_PADDING: f32 = 2.5;
        let mut units = [(0.0, 0.0); simon::PILES];
        for (i, u) in units.iter_mut().enumerate().take(simon::STORES) {
            *u = (DIST_X * i as f32, 1.2);
        }
        for i in 0..4 {
            units[simon::FOUNDATION + i] = ((i + 3) as f32 * DIST_X, 0.0);
        }
        let width_units = 9.0 * DIST_X + 1.0;
        let height_units = 1.2 + 1.0 + BOTTOM_PADDING;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < simon::STORES {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // KPatience distribue depuis le haut de l'écran.
        let deal_origin = Pos2::new(piles[0].origin.x, rect.top() - h);
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Clock::initialize` : les 8 piles sur deux rangées à
    /// gauche, le cadran des 12 fondations à droite, le tout centré.
    /// Positions en largeurs (x) et hauteurs (y) de carte.
    fn clock(rect: Rect, ratio: f32, game: &clock::Game) -> Layout {
        const DIST_X: f32 = 1.11;
        const YS: [f32; 12] = [0., 15., 52., 158., 264., 301., 316., 301., 264., 158., 52., 15.];
        const XS: [f32; 12] = [200., 280., 360., 400., 360., 280., 200., 120., 40., 0., 40., 120.];
        const BOTTOM_PADDING: f32 = 1.3;
        let mut units = [(0.0, 0.0); clock::PILES];
        for (i, u) in units.iter_mut().enumerate().take(clock::STORES) {
            *u = (DIST_X * (i % 4) as f32, 2.5 * (i / 4) as f32);
        }
        for i in 0..clock::HOURS {
            units[clock::FOUNDATION + i] = (4.0 * DIST_X + 0.4 + XS[i] / 72.0, 0.2 + YS[i] / 96.0);
        }
        let width_units = units.iter().map(|(x, _)| x + 1.0).fold(0.0, f32::max);
        let height_units =
            units.iter().enumerate().map(|(i, (_, y))| y + 1.0 + if i < clock::STORES { BOTTOM_PADDING } else { 0.0 }).fold(0.0, f32::max);
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        // Centré dans les deux sens (`AlignHCenter | AlignVCenter`).
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + (rect.height() - height_units * h) / 2.0;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < clock::STORES {
                    // La rangée du haut s'arrête au-dessus de celle du bas.
                    let limit = if i < 4 { y0 + 2.5 * h - LAYOUT_SPACING * h } else { bottom };
                    let avail = (limit - origin.y - h).max(h * 0.3);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, limit.max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        // Les cartes partent du centre du cadran.
        let deal_origin = piles[clock::FOUNDATION].origin.lerp(piles[clock::FOUNDATION + 6].origin, 0.5);
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Disposition de `Klondike::initialize` : le talon, la pioche (les 1
    /// ou 3 dernières cartes étalées) et les fondations en haut, les 7
    /// colonnes en dessous. Positions en largeurs (x) et hauteurs (y) de carte.
    fn klondike(rect: Rect, ratio: f32, game: &klondike::Game) -> Layout {
        const H: f32 = 1.0 / 6.0 + 0.02;
        const V: f32 = 0.25;
        let bottom_padding = SPREAD * 5.0;
        let mut units = [(0.0, 0.0); klondike::PILES];
        for (i, u) in units.iter_mut().enumerate().take(klondike::TABLEAU) {
            *u = ((1.0 + H) * i as f32, 1.0 + V);
        }
        units[klondike::WASTE] = (1.0 + H, 0.0);
        units[klondike::STOCK] = (0.0, 0.0);
        for i in 0..4 {
            units[klondike::FOUNDATION + i] = ((3 + i) as f32 * (1.0 + H), 0.0);
        }
        let width_units = 6.0 * (1.0 + H) + 1.0;
        let height_units = 1.0 + V + 1.0 + bottom_padding;
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;
        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                let card = Rect::from_min_size(origin, Vec2::new(w, h));
                match klondike::kind(i) {
                    klondike::PileKind::Tableau => {
                        let avail = (bottom - origin.y - h).max(h * 0.5);
                        PileGeom {
                            origin,
                            fan: Fan::Down,
                            gaps: fan_gaps(&game.piles[i], h, avail),
                            zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                        }
                    }
                    klondike::PileKind::Waste => {
                        PileGeom { origin, fan: Fan::Right { n: game.draw as usize, gap: SPREAD * w }, gaps: (0.0, 0.0), zone: card }
                    }
                    _ => PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: card },
                }
            })
            .collect();
        let deal_origin = piles[klondike::STOCK].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    /// Même calcul que `KCardScene::relayoutScene` de KPatience : la taille
    /// des cartes est la plus grande qui fait tenir, dans la fenêtre, les 10
    /// colonnes en largeur et, en hauteur, une carte et demie de plus pour
    /// les colonnes (`bottomPadding`), la rangée du bas et les marges.
    fn spider(rect: Rect, ratio: f32, game: &spider::Game) -> Layout {
        // En largeurs de carte : les colonnes et deux marges.
        let width_units = 9.0 * DIST_X + 1.0 + 2.0 * LAYOUT_MARGIN;
        // En hauteurs de carte : colonnes (1 + 1,5), espacement, rangée du bas, marges.
        let v_spacing = LAYOUT_SPACING * (1.0 + 1.0 / ratio) / 2.0;
        let height_units = 1.0 + 1.5 + v_spacing + 1.0 + 2.0 * LAYOUT_MARGIN;
        let w = (rect.width() / width_units).min(rect.height() / (height_units * ratio)).max(20.0);
        let h = w * ratio;

        let x0 = rect.left() + (rect.width() - w * (9.0 * DIST_X + 1.0)) / 2.0;
        let top = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h - h;

        // Les colonnes se resserrent pour ne pas descendre sur la rangée du bas.
        let avail = (bottom - v_spacing * h - top - h).max(h * 0.5);
        let piles = (0..spider::COLUMNS)
            .map(|i| {
                let x = x0 + i as f32 * DIST_X * w;
                PileGeom {
                    origin: Pos2::new(x, top),
                    fan: Fan::Down,
                    gaps: fan_gaps(&game.columns[i], h, avail),
                    zone: Rect::from_min_max(Pos2::new(x, top), Pos2::new(x + w, bottom)),
                }
            })
            .collect();
        let mut layout = Layout {
            card: Vec2::new(w, h),
            piles,
            deal_origin: Pos2::ZERO,
            spider: Some(SpiderGeom { left: x0, right: x0 + (spider::COLUMNS - 1) as f32 * DIST_X * w, bottom }),
        };
        layout.deal_origin = layout.stock_pos(ROWS - 1);
        layout
    }

    /// Disposition de `Freecell::initialize` : les cellules en haut à gauche,
    /// les fondations en haut à droite (ou en colonnes sur la droite s'il n'y
    /// a pas la place), les colonnes en dessous. Positions en largeurs (x) et
    /// hauteurs (y) de carte.
    fn freecell(rect: Rect, ratio: f32, game: &freecell::Game) -> Layout {
        let r = game.rules();
        let (stacks, cells, founds) = (r.stacks(), r.cells(), r.foundations());
        let decks = game.options.decks as f32;
        let right_foundations = cells + founds > stacks;
        let top_dist = if right_foundations { 1.13 } else { 1.08 };
        let bottom_dist = 1.13;
        let target_offset = ((stacks as f32 - 1.0) * bottom_dist + 1.0) - (3.0 * top_dist + 1.0) * decks;
        const TABLEAU_Y: f32 = 1.3;
        const BOTTOM_PADDING: f32 = 2.5;

        let mut units: Vec<(f32, f32)> = Vec::with_capacity(r.piles());
        units.extend((0..stacks).map(|i| (bottom_dist * i as f32, TABLEAU_Y)));
        units.extend((0..cells).map(|i| (top_dist * i as f32, 0.0)));
        let columns = cells.max(stacks) as f32;
        units.extend((0..founds).map(|i| {
            if right_foundations {
                (0.25 + columns * bottom_dist + (i / 4) as f32 * bottom_dist, bottom_dist * (i % 4) as f32)
            } else {
                (target_offset + top_dist * i as f32, 0.0)
            }
        }));
        let width_units = units.iter().map(|(x, _)| x + 1.0).fold(0.0, f32::max);
        let height_units =
            units.iter().enumerate().map(|(i, (_, y))| y + 1.0 + if i < stacks { BOTTOM_PADDING } else { 0.0 }).fold(0.0, f32::max);
        let w = (rect.width() / (width_units + 2.0 * LAYOUT_MARGIN))
            .min(rect.height() / ((height_units + 2.0 * LAYOUT_MARGIN) * ratio))
            .max(16.0);
        let h = w * ratio;
        let x0 = rect.left() + (rect.width() - width_units * w) / 2.0;
        let y0 = rect.top() + LAYOUT_MARGIN * h;
        let bottom = rect.bottom() - LAYOUT_MARGIN * h;

        let piles: Vec<PileGeom> = units
            .iter()
            .enumerate()
            .map(|(i, &(ux, uy))| {
                let origin = Pos2::new(x0 + ux * w, y0 + uy * h);
                if i < stacks {
                    let avail = (bottom - origin.y - h).max(h * 0.5);
                    PileGeom {
                        origin,
                        fan: Fan::Down,
                        gaps: fan_gaps(&game.piles[i], h, avail),
                        zone: Rect::from_min_max(origin, Pos2::new(origin.x + w, rect.bottom().max(origin.y + h))),
                    }
                } else {
                    PileGeom { origin, fan: Fan::None, gaps: (0.0, 0.0), zone: Rect::from_min_size(origin, Vec2::new(w, h)) }
                }
            })
            .collect();
        let deal_origin = piles[0].origin;
        Layout { card: Vec2::new(w, h), piles, deal_origin, spider: None }
    }

    fn card_pos(&self, game: &AnyGame, p: usize, index: usize) -> Pos2 {
        let g = &self.piles[p];
        let mut pos = g.origin;
        match g.fan {
            Fan::None => {}
            Fan::Down => {
                for c in &game.pile(p)[..index] {
                    pos.y += if c.face_up { g.gaps.1 } else { g.gaps.0 };
                }
            }
            Fan::Right { n, gap } => {
                let first = game.pile(p).len().saturating_sub(n);
                pos.x += index.saturating_sub(first) as f32 * gap;
            }
        }
        pos
    }

    fn slot_rect(&self, p: usize) -> Rect {
        Rect::from_min_size(self.piles[p].origin, self.card)
    }

    /// Spider : réserve, 5 tas qui se chevauchent, en bas à droite.
    fn stock_pos(&self, original_row: usize) -> Pos2 {
        let s = self.spider.as_ref().expect("Spider");
        let x = s.right - (ROWS - 1 - original_row) as f32 * DIST_X * self.card.x / 3.0;
        Pos2::new(x, s.bottom)
    }

    /// Spider : suites sorties, en bas à gauche.
    fn run_pos(&self, i: usize) -> Pos2 {
        let s = self.spider.as_ref().expect("Spider");
        Pos2::new(s.left + i as f32 * DIST_X * self.card.x / 3.0, s.bottom)
    }

    fn stock_rect(&self) -> Option<Rect> {
        self.spider.as_ref()?;
        Some(Rect::from_min_size(self.stock_pos(0), Vec2::new(self.stock_pos(ROWS - 1).x - self.stock_pos(0).x + self.card.x, self.card.y)))
    }
}

/// Position, face et profondeur de chaque carte.
struct Placed {
    card: Card,
    place: Place,
    pos: Pos2,
    z: i32,
}

fn place_cards(game: &AnyGame, layout: &Layout) -> Vec<Placed> {
    let mut out = Vec::with_capacity(160);
    for p in 0..game.pile_count() {
        let base = if game.pile_kind(p) == PileKind::Foundation { 100 } else { 200 };
        for (i, c) in game.pile(p).iter().enumerate() {
            out.push(Placed { card: *c, place: Place::Pile(p, i), pos: layout.card_pos(game, p, i), z: base + i as i32 });
        }
    }
    if let AnyGame::Spider(g) = game {
        let dealt = ROWS - g.stock.len();
        for (r, row) in g.stock.iter().enumerate() {
            let orig = dealt + r;
            for (k, c) in row.iter().enumerate() {
                // La prochaine rangée est au-dessus.
                out.push(Placed { card: *c, place: Place::Stock(r), pos: layout.stock_pos(orig), z: 100 - (orig as i32) * 12 + k as i32 });
            }
        }
        for (r, run) in g.runs.iter().enumerate() {
            for (k, c) in run.iter().enumerate() {
                // Le Roi (run[0]) visible au-dessus de sa suite.
                out.push(Placed { card: *c, place: Place::Run(r), pos: layout.run_pos(r), z: 20 + r as i32 * 14 + (13 - k as i32) });
            }
        }
    }
    out
}

// =================================================================== application

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Highlight {
    Card(u8),
    EmptyPile(usize),
}

#[derive(Clone, Copy)]
enum Action {
    NewDeal,
    Numbered(Variant, u32),
    Restart,
    /// « Nouvelle partie… » : l'écran de choix du jeu.
    ChooseGame,
    SetSuits(u8),
    SetFaceUp(bool),
    SetFreeCell(freecell::Options),
    SetBakers(bakers::Options),
    SetCastle(castle::Options),
    SetKlondikeDraw(u8),
}

enum Dialog {
    None,
    Numbered { variant: Variant, deal: u32 },
    Stats { variant: Variant },
    Confirm { action: Action, dont_ask: bool },
    About,
    Rules,
}

struct Drag {
    from: usize,
    index: usize,
    /// Position du pointeur par rapport au coin de la première carte.
    grab: Vec2,
}

pub struct SpiderApp {
    settings: Settings,
    stats: Stats,
    game: AnyGame,
    help_used: bool,
    recorded: bool,

    decks: Vec<AssetInfo>,
    themes: Vec<AssetInfo>,
    deck: Arc<Svg>,
    theme: Arc<Svg>,
    /// Hauteur / largeur des cartes.
    ratio: f32,
    cards: Rendered,
    decor: Rendered,

    shown: HashMap<u8, Pos2>,
    drag: Option<Drag>,
    highlights: HashSet<Highlight>,
    hint_waiting: bool,
    solver: SolverManager,
    demo: bool,
    next_demo: f64,
    /// FreeCell : « Déposer » demandé (dépôt automatique désactivé).
    dropping: bool,
    /// Écran de choix du jeu (comme `GameSelectionScene` de KPatience) ;
    /// `game` n'y est pas affiché.
    selecting: bool,
    selection: Selection,
    dialog: Dialog,
    title: String,
    error: Option<String>,
}

fn load_or_default(list: &[AssetInfo], id: &str, load: fn(&AssetInfo) -> Result<Svg, String>, default: &str) -> (Svg, Option<String>) {
    let info = list.iter().find(|a| a.id == id).or_else(|| list.iter().find(|a| a.id == default)).expect("élément intégré");
    match load(info) {
        Ok(svg) => (svg, None),
        Err(e) => {
            let fallback = list.iter().find(|a| a.svg.is_none()).expect("élément intégré");
            (load(fallback).expect("SVG intégré valide"), Some(e))
        }
    }
}

/// Une nouvelle partie du jeu choisi dans les réglages.
fn new_game(settings: &Settings, deal: u32) -> AnyGame {
    match settings.game {
        GameKind::Spider => AnyGame::Spider(spider::Game::new(settings.suit_count, deal, settings.stacks_face_up)),
        GameKind::FreeCell => AnyGame::FreeCell(freecell::Game::new(settings.freecell, deal)),
        GameKind::Klondike => AnyGame::Klondike(klondike::Game::new(settings.klondike_draw, deal)),
        GameKind::Clock => AnyGame::Clock(clock::Game::new(deal)),
        GameKind::Simon => AnyGame::Simon(simon::Game::new(deal)),
        GameKind::Golf => AnyGame::Golf(golf::Game::new(deal)),
        GameKind::Yukon => AnyGame::Yukon(yukon::Game::new(deal)),
        GameKind::Grandfather => AnyGame::Grandfather(grandf::Game::new(deal)),
        GameKind::Mod3 => AnyGame::Mod3(mod3::Game::new(deal)),
        GameKind::AcesUp => AnyGame::AcesUp(aces::Game::new(deal)),
        GameKind::FortyEight => AnyGame::FortyEight(fortyeight::Game::new(deal)),
        GameKind::Bakers => AnyGame::Bakers(bakers::Game::new(settings.bakers, deal)),
        GameKind::Castle => AnyGame::Castle(castle::Game::new(settings.castle, deal)),
        GameKind::Normandy => AnyGame::Normandy(normandy::Game::new(deal)),
    }
}

/// Choisit le jeu et la variante dans les réglages.
fn apply_variant(settings: &mut Settings, variant: Variant) {
    match variant {
        Variant::Spider(s) => {
            settings.game = GameKind::Spider;
            settings.suit_count = s;
        }
        Variant::FreeCell(v) => {
            settings.game = GameKind::FreeCell;
            if let Some((_, o)) = freecell::PRESETS.get(v) {
                settings.freecell = *o;
            }
        }
        Variant::Klondike(d) => {
            settings.game = GameKind::Klondike;
            settings.klondike_draw = d;
        }
        Variant::Clock => settings.game = GameKind::Clock,
        Variant::Simon => settings.game = GameKind::Simon,
        Variant::Golf => settings.game = GameKind::Golf,
        Variant::Yukon => settings.game = GameKind::Yukon,
        Variant::Grandfather => settings.game = GameKind::Grandfather,
        Variant::Mod3 => settings.game = GameKind::Mod3,
        Variant::AcesUp => settings.game = GameKind::AcesUp,
        Variant::FortyEight => settings.game = GameKind::FortyEight,
        Variant::Bakers(v) => {
            settings.game = GameKind::Bakers;
            if let Some((_, o)) = bakers::PRESETS.get(v) {
                settings.bakers = *o;
            }
        }
        Variant::Normandy => settings.game = GameKind::Normandy,
        Variant::Castle(v) => {
            settings.game = GameKind::Castle;
            if let Some((_, o)) = castle::PRESETS.get(v) {
                // La disposition est un choix d'affichage : elle reste.
                settings.castle = castle::Options { modern: settings.castle.modern, ..*o };
            }
        }
    }
}

impl SpiderApp {
    /// `start` : (variante, numéro de donne) demandés en ligne de commande.
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<(Option<Variant>, u32)>) -> SpiderApp {
        let mut settings = Settings::load();
        if let Some((Some(variant), _)) = start {
            apply_variant(&mut settings, variant);
        }
        let decks = assets::find_decks();
        let themes = assets::find_themes();
        let (deck, e1) = load_or_default(&decks, &settings.card_deck, Svg::load_deck, assets::DEFAULT_DECK);
        let (theme, e2) = load_or_default(&themes, &settings.theme, Svg::load_theme, assets::DEFAULT_THEME);
        let ratio = deck.element_size("back").map_or(1.4, |(w, h)| h / w);

        // Comme KPatience : la partie demandée, sinon celle qui a été
        // gardée, sinon l'écran de choix du jeu.
        let (game, help_used, selecting) = match (start, settings.remember_state, storage::load_state()) {
            (Some((_, deal)), _, _) => (new_game(&settings, deal), false, false),
            (None, true, Some(s)) if !s.game.is_won() => (s.game, s.help_used, false),
            _ => (new_game(&settings, random_deal()), false, true),
        };
        let theme_arc = Arc::new(theme);
        settings.game = game.kind();

        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let mut app = SpiderApp {
            settings,
            stats: Stats::load(),
            game,
            help_used,
            recorded: false,
            decks,
            themes,
            deck: Arc::new(deck),
            theme: theme_arc.clone(),
            ratio,
            cards: Rendered::new(),
            decor: Rendered::new(),
            shown: HashMap::new(),
            drag: None,
            highlights: HashSet::new(),
            hint_waiting: false,
            solver: SolverManager::new(),
            demo: false,
            next_demo: 0.0,
            dropping: false,
            selecting,
            selection: Selection::new(&theme_arc),
            dialog: Dialog::None,
            title: String::new(),
            error: e1.or(e2),
        };
        let now = app.now();
        let solver_on = app.settings.solver_enabled && !app.selecting;
        app.solver.position_changed(&app.game, None, solver_on, now);
        app
    }

    // ------------------------------------------------------------ partie

    fn game_in_progress(&self) -> bool {
        self.game.moves() > 0 && !self.game.is_won() && !self.recorded
    }

    fn record_if_in_progress(&mut self) {
        if self.game_in_progress() {
            self.stats.record(self.game.variant(), false, self.game.moves());
            self.recorded = true;
        }
    }

    fn request(&mut self, action: Action) {
        if self.game_in_progress() && self.settings.confirm_abandon {
            self.dialog = Dialog::Confirm { action, dont_ask: false };
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Action) {
        if let Action::Restart = action {
            // Comme KPatience : recommencer la même donne ne compte pas comme une défaite.
            self.start(self.game.restarted(), false);
            return;
        }
        self.record_if_in_progress();
        let deal = self.game.deal_number();
        let deal = match action {
            Action::NewDeal => random_deal(),
            Action::Numbered(v, d) => {
                apply_variant(&mut self.settings, v);
                d
            }
            Action::ChooseGame => {
                self.show_selection();
                return;
            }
            Action::SetSuits(s) => {
                self.settings.suit_count = s;
                deal
            }
            Action::SetFaceUp(f) => {
                self.settings.stacks_face_up = f;
                deal
            }
            Action::SetFreeCell(o) => {
                self.settings.freecell = o.sanitized();
                deal
            }
            Action::SetBakers(o) => {
                self.settings.bakers = o;
                deal
            }
            Action::SetCastle(o) => {
                self.settings.castle = o.sanitized();
                deal
            }
            Action::SetKlondikeDraw(d) => {
                self.settings.klondike_draw = d;
                deal
            }
            Action::Restart => unreachable!(),
        };
        self.settings.save();
        self.start(new_game(&self.settings, deal), true);
    }

    fn start(&mut self, game: AnyGame, reset_recorded: bool) {
        self.game = game;
        self.selecting = false;
        if reset_recorded {
            self.recorded = false;
            self.help_used = false;
            self.solver.ever_winnable = false;
        }
        self.demo = false;
        self.dropping = false;
        self.drag = None;
        self.clear_hint();
        // Les cartes partent de la réserve (ou de la première colonne), pour
        // l'animation de la distribution.
        self.shown.clear();
        self.solver.cache.clear();
        self.changed(None);
    }

    /// Passe à l'écran de choix du jeu (`MainWindow::slotShowGameSelectionScreen`) :
    /// la partie en cours est abandonnée.
    fn show_selection(&mut self) {
        self.record_if_in_progress();
        self.recorded = true;
        self.selecting = true;
        self.demo = false;
        self.dropping = false;
        self.drag = None;
        self.dialog = Dialog::None;
        self.clear_hint();
        let now = self.now();
        self.solver.position_changed(&self.game, None, false, now);
        self.selection.index = None;
        storage::clear_state();
    }

    /// Un jeu a été choisi : une donne au hasard (`MainWindow::slotGameSelected`).
    fn game_selected(&mut self, kind: GameKind) {
        self.settings.game = kind;
        self.settings.save();
        self.start(new_game(&self.settings, random_deal()), true);
    }

    /// À appeler après chaque changement de la position.
    fn changed(&mut self, played: Option<Move>) {
        self.clear_hint();
        let now = self.now();
        self.solver.position_changed(&self.game, played, self.settings.solver_enabled, now);
        if self.game.is_won() {
            self.demo = false;
            self.dropping = false;
            if !self.recorded {
                self.stats.record(self.game.variant(), true, self.game.moves());
                self.recorded = true;
            }
            storage::clear_state();
        }
    }

    fn now(&self) -> f64 {
        // `Instant` relatif au démarrage, comme le temps d'egui.
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64()
    }

    fn play(&mut self, m: Move) -> bool {
        if self.game.play(m) {
            self.changed(Some(m));
            true
        } else {
            false
        }
    }

    fn undo(&mut self) {
        self.demo = false;
        self.dropping = false;
        if self.game.undo() {
            self.changed(None);
        }
    }

    fn redo(&mut self) {
        self.demo = false;
        if self.game.redo() {
            self.changed(None);
        }
    }

    fn deal(&mut self) {
        if self.game.can_deal() {
            self.play(Move::Deal);
        }
    }

    /// Double-clic : la carte (ou la suite) va à la meilleure place.
    fn auto_move(&mut self, p: usize, index: usize) {
        if let Some(m) = self.game.auto_move(p, index) {
            self.play(m);
        }
    }

    /// FreeCell : « Déposer » joue les sorties sans risque vers les fondations.
    fn can_drop_now(&self) -> bool {
        !self.settings.autodrop && !self.game.is_won() && self.game.autodrop().is_some()
    }

    fn drop_to_foundations(&mut self) {
        if self.can_drop_now() {
            self.dropping = true;
        }
    }

    /// Dépôt automatique (comme KPatience : pas pendant la démonstration, ni
    /// quand on peut refaire un coup annulé, sinon on ne pourrait plus
    /// annuler une sortie) ou « Déposer » demandé.
    fn step_autodrop(&mut self, busy: bool) {
        if busy || self.demo || self.game.is_won() {
            return;
        }
        let auto = self.settings.autodrop && !self.game.can_redo();
        if !auto && !self.dropping {
            return;
        }
        match self.game.autodrop() {
            Some(m) => {
                if self.play(m) && self.game.draw_after_drop(m) {
                    self.play(Move::Deal);
                }
            }
            None => self.dropping = false,
        }
    }

    // ------------------------------------------------------------ conseils

    fn clear_hint(&mut self) {
        self.highlights.clear();
        self.hint_waiting = false;
    }

    fn hint_active(&self) -> bool {
        self.hint_waiting || !self.highlights.is_empty()
    }

    fn toggle_hint(&mut self) {
        if self.hint_active() {
            self.clear_hint();
        } else if !self.game.is_won() {
            self.help_used = true;
            self.show_hint();
        }
    }

    fn highlight_move(&mut self, m: Move) {
        match m {
            Move::Cards { from, index, to } => {
                self.highlights.insert(Highlight::Card(self.game.pile(from)[index].id));
                match self.game.pile(to).last() {
                    Some(top) => self.highlights.insert(Highlight::Card(top.id)),
                    None => self.highlights.insert(Highlight::EmptyPile(to)),
                };
            }
            Move::Deal => {
                self.highlight_stock();
            }
        }
    }

    /// Surligne la réserve : au Spider, la carte visible du prochain tas ; au
    /// Klondike, le talon (ou sa place, s'il faut retourner la pioche).
    fn highlight_stock(&mut self) {
        if let AnyGame::Spider(g) = &self.game
            && let Some(top) = g.stock.first().and_then(|row| row.last())
        {
            self.highlights.insert(Highlight::Card(top.id));
        }
        if let Some(p) = self.game.stock_pile() {
            match self.game.pile(p).last() {
                Some(top) => self.highlights.insert(Highlight::Card(top.id)),
                None => self.highlights.insert(Highlight::EmptyPile(p)),
            };
        }
    }

    /// Le coup gagnant si l'outil de résolution le connaît ; sinon, s'il
    /// calcule encore, on attend son résultat ; sinon, tous les coups utiles.
    fn show_hint(&mut self) {
        self.highlights.clear();
        if let Some(m) = self.solver.next_move() {
            self.hint_waiting = false;
            self.highlight_move(m);
            return;
        }
        if self.solver.calculating() {
            self.hint_waiting = true;
            return;
        }
        self.hint_waiting = false;
        let moves = self.game.sensible_moves();
        for m in &moves {
            if let Move::Cards { from, index, .. } = *m {
                self.highlights.insert(Highlight::Card(self.game.pile(from)[index].id));
            }
        }
        if moves.is_empty() {
            self.highlight_stock();
        }
    }

    fn toggle_demo(&mut self) {
        self.demo = !self.demo && !self.game.is_won();
        if self.demo {
            self.help_used = true;
            self.next_demo = 0.0;
        }
    }

    fn step_demo(&mut self, now: f64, animating: bool) {
        if !self.demo || animating || now < self.next_demo {
            return;
        }
        if let Some(m) = self.solver.next_move() {
            self.play(m);
            self.next_demo = now + DEMO_DELAY;
        } else if !self.solver.calculating() {
            // Pas de chemin gagnant connu : la démonstration s'arrête.
            self.demo = false;
        }
    }

    // ------------------------------------------------------------ réglages

    fn set_deck(&mut self, id: &str) {
        let Some(info) = self.decks.iter().find(|d| d.id == id).cloned() else { return };
        match Svg::load_deck(&info) {
            Ok(svg) => {
                self.ratio = svg.element_size("back").map_or(1.4, |(w, h)| h / w);
                self.deck = Arc::new(svg);
                self.cards.reset();
                self.settings.card_deck = info.id;
                self.settings.save();
            }
            Err(e) => self.error = Some(format!("Jeu de cartes « {} » illisible : {e}", info.name)),
        }
    }

    fn set_theme(&mut self, id: &str) {
        let Some(info) = self.themes.iter().find(|t| t.id == id).cloned() else { return };
        match Svg::load_theme(&info) {
            Ok(svg) => {
                self.theme = Arc::new(svg);
                self.decor.reset();
                self.selection = Selection::new(&self.theme);
                self.settings.theme = info.id;
                self.settings.save();
            }
            Err(e) => self.error = Some(format!("Thème « {} » illisible : {e}", info.name)),
        }
    }

    fn set_solver_enabled(&mut self, enabled: bool) {
        self.settings.solver_enabled = enabled;
        self.settings.save();
        let now = self.now();
        self.solver.position_changed(&self.game, None, enabled, now);
    }

    // ------------------------------------------------------------ menus

    fn shortcuts(&mut self, ctx: &Context) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }
        let pressed = |ctx: &Context, m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if pressed(ctx, Modifiers::COMMAND, Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) && !self.selecting {
            self.redo();
        }
        if pressed(ctx, Modifiers::COMMAND, Key::Z) && !self.selecting {
            self.undo();
        }
        if pressed(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::N) && !self.selecting {
            self.request(Action::ChooseGame);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::N) && !self.selecting {
            self.request(Action::NewDeal);
        }
        if pressed(ctx, Modifiers::COMMAND, Key::D) {
            self.dialog = Dialog::Numbered { variant: self.game.variant(), deal: self.game.deal_number() };
        }
        if self.selecting {
            self.selection_keys(ctx);
            return;
        }
        if pressed(ctx, Modifiers::NONE, Key::F1) {
            self.dialog = Dialog::Rules;
        }
        if pressed(ctx, Modifiers::NONE, Key::F5) {
            self.request(Action::Restart);
        }
        if pressed(ctx, Modifiers::NONE, Key::H) {
            self.toggle_hint();
        }
        if pressed(ctx, Modifiers::NONE, Key::D) {
            self.toggle_demo();
        }
        if pressed(ctx, Modifiers::NONE, Key::Enter) && matches!(self.game.kind(), GameKind::Spider | GameKind::Mod3 | GameKind::AcesUp | GameKind::Normandy) {
            self.deal();
        }
        if pressed(ctx, Modifiers::NONE, Key::Tab) && matches!(self.game.kind(), GameKind::Klondike | GameKind::Golf | GameKind::FortyEight) {
            self.deal();
        }
        if pressed(ctx, Modifiers::NONE, Key::R) && self.game.kind() == GameKind::Grandfather {
            self.deal();
        }
        if pressed(ctx, Modifiers::NONE, Key::P) {
            self.drop_to_foundations();
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Jeu", |ui| {
                let in_game = !self.selecting;
                if ui.add_enabled(in_game, egui::Button::new("Nouvelle partie…").shortcut_text("Ctrl+Maj+N")).clicked() {
                    self.request(Action::ChooseGame);
                }
                if ui.add_enabled(in_game, egui::Button::new("Nouvelle donne").shortcut_text("Ctrl+N")).clicked() {
                    self.request(Action::NewDeal);
                }
                if ui.add(egui::Button::new("Nouvelle donne numérotée…").shortcut_text("Ctrl+D")).clicked() {
                    self.dialog = Dialog::Numbered { variant: self.game.variant(), deal: self.game.deal_number() };
                }
                if ui.add_enabled(in_game, egui::Button::new("Recommencer la donne").shortcut_text("F5")).clicked() {
                    self.request(Action::Restart);
                }
                ui.separator();
                if ui.button("Statistiques").clicked() {
                    self.dialog = Dialog::Stats { variant: self.game.variant() };
                }
                ui.separator();
                if ui.add(egui::Button::new("Quitter").shortcut_text("Ctrl+Q")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Déplacer", |ui| {
                if self.selecting {
                    ui.disable();
                }
                if ui.add_enabled(self.game.can_undo(), egui::Button::new("Annuler").shortcut_text("Ctrl+Z")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(self.game.can_redo(), egui::Button::new("Refaire").shortcut_text("Ctrl+Maj+Z")).clicked() {
                    self.redo();
                }
                ui.separator();
                let playing = !self.game.is_won();
                if ui.add_enabled(playing, egui::Button::selectable(self.hint_active(), "Conseil").shortcut_text("H")).clicked() {
                    self.toggle_hint();
                }
                if ui.add_enabled(playing, egui::Button::selectable(self.demo, "Démonstration").shortcut_text("D")).clicked() {
                    self.toggle_demo();
                }
                match self.game.kind() {
                    GameKind::Spider | GameKind::Mod3 | GameKind::AcesUp | GameKind::Normandy => {
                        if ui
                            .add_enabled(self.game.can_deal(), egui::Button::new("Distribuer une rangée").shortcut_text("Entrée"))
                            .clicked()
                        {
                            self.deal();
                        }
                    }
                    GameKind::FreeCell | GameKind::Clock | GameKind::Simon | GameKind::Yukon | GameKind::Bakers | GameKind::Castle => {}
                    GameKind::Klondike | GameKind::Golf | GameKind::FortyEight => {
                        if ui.add_enabled(self.game.can_deal(), egui::Button::new("Piocher").shortcut_text("Tab")).clicked() {
                            self.deal();
                        }
                    }
                    GameKind::Grandfather => {
                        if ui.add_enabled(self.game.can_deal(), egui::Button::new("Redistribuer").shortcut_text("R")).clicked() {
                            self.deal();
                        }
                    }
                }
                if self.game.uses_autodrop()
                    && ui.add_enabled(self.can_drop_now(), egui::Button::new("Déposer sur les fondations").shortcut_text("P")).clicked()
                {
                    self.drop_to_foundations();
                }
            });
            ui.menu_button("Configuration", |ui| {
                if !self.selecting && !matches!(
                        self.game.kind(),
                        GameKind::Clock | GameKind::Simon | GameKind::Golf | GameKind::Yukon | GameKind::Grandfather | GameKind::Mod3 | GameKind::AcesUp | GameKind::FortyEight | GameKind::Normandy
                    ) {
                    match &self.game {
                        AnyGame::Spider(_) => self.spider_options(ui),
                        AnyGame::FreeCell(g) => {
                            let options = g.options;
                            self.freecell_options(ui, options);
                        }
                        AnyGame::Bakers(g) => {
                            let options = g.bakers;
                            self.bakers_options(ui, options);
                        }
                        AnyGame::Castle(g) => {
                            let options = g.castle;
                            self.castle_options(ui, options);
                        }
                        AnyGame::Klondike(g) => {
                            let draw = g.draw;
                            ui.menu_button("Pioche", |ui| {
                                for (d, label) in [(1, "Piocher 1 carte (plus facile)"), (3, "Piocher 3 cartes (plus difficile)")] {
                                    if ui.radio(draw == d, label).clicked() && draw != d {
                                        self.request(Action::SetKlondikeDraw(d));
                                    }
                                }
                            });
                        }
                        // Pas d'option pour les autres jeux.
                        AnyGame::Clock(_)
                        | AnyGame::Simon(_)
                        | AnyGame::Golf(_)
                        | AnyGame::Yukon(_)
                        | AnyGame::Grandfather(_)
                        | AnyGame::Mod3(_)
                        | AnyGame::AcesUp(_)
                        | AnyGame::FortyEight(_)
                        | AnyGame::Normandy(_) => {}
                    }
                    ui.separator();
                }
                let mut solver = self.settings.solver_enabled;
                if ui.checkbox(&mut solver, "Activer l'outil de résolution").changed() {
                    self.set_solver_enabled(solver);
                }
                if self.game.uses_autodrop()
                    && !self.selecting
                    && ui.checkbox(&mut self.settings.autodrop, "Déposer automatiquement sur les fondations").changed()
                {
                    self.dropping = false;
                    self.settings.save();
                }
                if ui.checkbox(&mut self.settings.remember_state, "Se souvenir de l'état de la partie à sa fermeture").changed() {
                    self.settings.save();
                }
                if ui.checkbox(&mut self.settings.confirm_abandon, "Demander avant d'abandonner une partie").changed() {
                    self.settings.save();
                }
                ui.menu_button("Barre d'outils", |ui| {
                    if ui.checkbox(&mut self.settings.show_toolbar, "Afficher la barre d'outils").changed() {
                        self.settings.save();
                    }
                    ui.separator();
                    for (t, label) in [(false, "Icônes seules"), (true, "Icônes et texte")] {
                        if ui
                            .add_enabled(self.settings.show_toolbar, egui::RadioButton::new(self.settings.toolbar_text == t, label))
                            .clicked()
                        {
                            self.settings.toolbar_text = t;
                            self.settings.save();
                        }
                    }
                });
                ui.separator();
                ui.menu_button("Jeu de cartes", |ui| {
                    let mut chosen = None;
                    for d in &self.decks {
                        if ui.radio(self.settings.card_deck == d.id, &d.name).clicked() {
                            chosen = Some(d.id.clone());
                        }
                    }
                    if let Some(id) = chosen {
                        self.set_deck(&id);
                    }
                });
                ui.menu_button("Thème", |ui| {
                    let mut chosen = None;
                    for t in &self.themes {
                        if ui.radio(self.settings.theme == t.id, &t.name).clicked() {
                            chosen = Some(t.id.clone());
                        }
                    }
                    if let Some(id) = chosen {
                        self.set_theme(&id);
                    }
                });
            });
            ui.menu_button("Aide", |ui| {
                if self.selecting {
                    ui.disable();
                }
                let rules = match self.game.kind() {
                    GameKind::Spider => "Règles du Spider",
                    GameKind::FreeCell => "Règles du FreeCell",
                    GameKind::Klondike => "Règles du Klondike",
                    GameKind::Clock => "Règles de l'horloge du grand-père",
                    GameKind::Simon => "Règles de Simon le simple",
                    GameKind::Golf => "Règles du Golf",
                    GameKind::Yukon => "Règles du Yukon",
                    GameKind::Grandfather => "Règles de Grand-père",
                    GameKind::Mod3 => "Règles de Mod3",
                    GameKind::AcesUp => "Règles des As",
                    GameKind::FortyEight => "Règles de Quarante & Huit",
                    GameKind::Bakers => "Règles de Treize à la douzaine",
                    GameKind::Castle => "Règles de Château",
                    GameKind::Normandy => "Règles de Normandie",
                };
                if ui.add(egui::Button::new(rules).shortcut_text("F1")).clicked() {
                    self.dialog = Dialog::Rules;
                }
                if ui.button(format!("À propos de {APP_NAME}")).clicked() {
                    self.dialog = Dialog::About;
                }
            });
        });
    }

    fn spider_options(&mut self, ui: &mut egui::Ui) {
        let AnyGame::Spider(g) = &self.game else { return };
        let (suits, face_up) = (g.suit_count, g.stacks_face_up);
        ui.menu_button("Couleurs", |ui| {
            for (n, label) in
                [(1, "1 couleur (facile)"), (2, "2 couleurs (moyen)"), (3, "3 couleurs (difficile)"), (4, "4 couleurs (très difficile)")]
            {
                if ui.radio(suits == n, label).clicked() && suits != n {
                    self.request(Action::SetSuits(n));
                }
            }
        });
        ui.menu_button("Cartes des colonnes", |ui| {
            for (f, label) in [(false, "Faces cachées (plus difficile)"), (true, "Faces visibles (plus facile)")] {
                if ui.radio(face_up == f, label).clicked() && face_up != f {
                    self.request(Action::SetFaceUp(f));
                }
            }
        });
    }

    /// Les options du FreeCell, comme le menu de KPatience : variantes
    /// connues, puis chaque règle (ce qui donne une variante personnalisée).
    fn freecell_options(&mut self, ui: &mut egui::Ui, current: freecell::Options) {
        let mut chosen: Option<freecell::Options> = None;
        ui.menu_button("Variante", |ui| {
            let variant = current.variant();
            for (i, (name, o)) in freecell::PRESETS.iter().enumerate() {
                if ui.radio(variant == i, *name).clicked() {
                    chosen = Some(*o);
                }
            }
            ui.add_enabled(false, egui::RadioButton::new(variant == freecell::CUSTOM, "Personnalisée"));
        });
        ui.menu_button("Colonnes vides", |ui| {
            for (f, label) in [
                (EmptyFill::Any, "N'importe quelle carte (facile)"),
                (EmptyFill::Kings, "Rois seulement (moyen)"),
                (EmptyFill::None, "Aucune carte (difficile)"),
            ] {
                if ui.radio(current.empty_fill == f, label).clicked() {
                    chosen = Some(freecell::Options { empty_fill: f, ..current });
                }
            }
        });
        ui.menu_button("Suites", |ui| {
            for (b, label) in
                [(Build::AlternateColor, "Couleurs alternées (rouge, noir)"), (Build::Suit, "Même couleur"), (Build::Rank, "Rang seul")]
            {
                if ui.radio(current.build == b, label).clicked() {
                    chosen = Some(freecell::Options { build: b, ..current });
                }
            }
        });
        ui.menu_button("Cellules libres", |ui| {
            for n in 0..=8 {
                if ui.radio(current.cells == n, n.to_string()).clicked() {
                    chosen = Some(freecell::Options { cells: n, ..current });
                }
            }
        });
        ui.menu_button("Colonnes", |ui| {
            for n in 6..=12 {
                if ui.radio(current.stacks == n, n.to_string()).clicked() {
                    chosen = Some(freecell::Options { stacks: n, ..current });
                }
            }
        });
        ui.menu_button("Paquets", |ui| {
            for n in 1..=3 {
                let label = if n == 1 { "1 paquet (52 cartes)".to_string() } else { format!("{n} paquets ({} cartes)", 52 * n as u32) };
                if ui.radio(current.decks == n, label).clicked() {
                    chosen = Some(freecell::Options { decks: n, ..current });
                }
            }
        });
        if let Some(o) = chosen.filter(|o| *o != current) {
            self.request(Action::SetFreeCell(o));
        }
    }

    fn bakers_options(&mut self, ui: &mut egui::Ui, current: bakers::Options) {
        let mut chosen: Option<bakers::Options> = None;
        ui.menu_button("Variante", |ui| {
            let variant = current.variant();
            for (i, (name, o)) in bakers::PRESETS.iter().enumerate() {
                if ui.radio(variant == i, *name).clicked() {
                    chosen = Some(*o);
                }
            }
            ui.add_enabled(false, egui::RadioButton::new(variant == bakers::CUSTOM, "Personnalisée"));
        });
        ui.menu_button("Colonnes vides", |ui| {
            for (f, label) in [
                (EmptyFill::Any, "N'importe quelle carte (facile)"),
                (EmptyFill::Kings, "Rois seulement (moyen)"),
                (EmptyFill::None, "Aucune carte (difficile)"),
            ] {
                if ui.radio(current.empty_fill == f, label).clicked() {
                    chosen = Some(bakers::Options { empty_fill: f, ..current });
                }
            }
        });
        ui.menu_button("Colonnes", |ui| {
            for (d, label) in [(false, "Face visible (plus facile)"), (true, "Face cachée (plus difficile)")] {
                if ui.radio(current.face_down == d, label).clicked() {
                    chosen = Some(bakers::Options { face_down: d, ..current });
                }
            }
        });
        ui.menu_button("Suites", |ui| {
            for (b, label) in
                [(Build::AlternateColor, "Couleurs alternées (rouge, noir)"), (Build::Suit, "Même couleur"), (Build::Rank, "Rang seul")]
            {
                if ui.radio(current.build == b, label).clicked() {
                    chosen = Some(bakers::Options { build: b, ..current });
                }
            }
        });
        if let Some(o) = chosen.filter(|o| *o != current) {
            self.request(Action::SetBakers(o));
        }
    }

    fn castle_options(&mut self, ui: &mut egui::Ui, current: castle::Options) {
        use castle::{Faces, Prefill};
        let mut chosen: Option<castle::Options> = None;
        ui.menu_button("Variante", |ui| {
            let variant = current.variant();
            for (i, (name, o)) in castle::PRESETS.iter().enumerate() {
                if ui.radio(variant == i, *name).clicked() {
                    chosen = Some(castle::Options { faces: current.faces, modern: current.modern, ..*o });
                }
            }
            ui.add_enabled(false, egui::RadioButton::new(variant == castle::CUSTOM, "Personnalisée"));
        });
        ui.menu_button("Colonnes vides", |ui| {
            for (f, label) in [
                (EmptyFill::Any, "N'importe quelle carte (facile)"),
                (EmptyFill::Kings, "Rois seulement (moyen)"),
                (EmptyFill::None, "Aucune carte (difficile)"),
            ] {
                if ui.radio(current.empty_fill == f, label).clicked() {
                    chosen = Some(castle::Options { empty_fill: f, ..current });
                }
            }
        });
        ui.menu_button("Suites", |ui| {
            for (b, label) in
                [(Build::AlternateColor, "Couleurs alternées (rouge, noir)"), (Build::Suit, "Même couleur"), (Build::Rank, "Rang seul")]
            {
                if ui.radio(current.build == b, label).clicked() {
                    chosen = Some(castle::Options { build: b, ..current });
                }
            }
        });
        ui.menu_button("Réserves", |ui| {
            for n in 0..=4 {
                if ui.radio(current.reserves == n, n.to_string()).clicked() {
                    chosen = Some(castle::Options { reserves: n, ..current });
                }
            }
        });
        ui.menu_button("Colonnes", |ui| {
            for n in 6..=10 {
                if ui.radio(current.stacks == n, n.to_string()).clicked() {
                    chosen = Some(castle::Options { stacks: n, ..current });
                }
            }
        });
        ui.menu_button("Cartes des colonnes", |ui| {
            for (f, label) in [(Faces::Down, "Face cachée"), (Faces::Up, "Face visible"), (Faces::Alternate, "Une rangée sur deux visible")] {
                if ui.radio(current.faces == f, label).clicked() {
                    chosen = Some(castle::Options { faces: f, ..current });
                }
            }
        });
        ui.menu_button("Fondations à la distribution", |ui| {
            for (f, label) in [
                (Prefill::None, "Aucune carte"),
                (Prefill::Aces, "Les As"),
                (Prefill::Any, "Les As et les cartes qui les suivent"),
            ] {
                if ui.radio(current.prefill == f, label).clicked() {
                    chosen = Some(castle::Options { prefill: f, ..current });
                }
            }
        });
        ui.menu_button("Disposition", |ui| {
            for (m, label) in [(false, "Classique"), (true, "Moderne")] {
                if ui.radio(current.modern == m, label).clicked() {
                    chosen = Some(castle::Options { modern: m, ..current });
                }
            }
        });
        if let Some(o) = chosen.filter(|o| *o != current) {
            self.request(Action::SetCastle(o));
        }
    }

    /// Barre d'outils, comme celle de KPatience.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let g = !self.selecting;
            let playing = g && !self.game.is_won();
            let text = self.settings.toolbar_text;
            if tool_button(ui, text, Icon::New, "Nouvelle partie", "Nouvelle partie… (Ctrl+Maj+N)", g, false).clicked() {
                self.request(Action::ChooseGame);
            }
            if tool_button(ui, text, Icon::Shuffle, "Nouvelle donne", "Nouvelle donne (Ctrl+N)", g, false).clicked() {
                self.request(Action::NewDeal);
            }
            if tool_button(ui, text, Icon::Restart, "Recommencer", "Recommencer la donne (F5)", g, false).clicked() {
                self.request(Action::Restart);
            }
            ui.separator();
            if tool_button(ui, text, Icon::Undo, "Annuler", "Annuler (Ctrl+Z)", g && self.game.can_undo(), false).clicked() {
                self.undo();
            }
            if tool_button(ui, text, Icon::Redo, "Refaire", "Refaire (Ctrl+Maj+Z)", g && self.game.can_redo(), false).clicked() {
                self.redo();
            }
            ui.separator();
            if tool_button(ui, text, Icon::Hint, "Conseil", "Conseil (H)", playing, self.hint_active()).clicked() {
                self.toggle_hint();
            }
            let demo_icon = if self.demo { Icon::Pause } else { Icon::Demo };
            if tool_button(ui, text, demo_icon, "Démonstration", "Démonstration (D)", playing, self.demo).clicked() {
                self.toggle_demo();
            }
            match self.game.kind() {
                GameKind::Spider | GameKind::Mod3 | GameKind::AcesUp | GameKind::Normandy => {
                    if tool_button(ui, text, Icon::Deal, "Distribuer", "Distribuer une rangée (Entrée)", g && self.game.can_deal(), false)
                        .clicked()
                    {
                        self.deal();
                    }
                }
                GameKind::FreeCell | GameKind::Clock | GameKind::Simon | GameKind::Yukon | GameKind::Bakers | GameKind::Castle => {}
                GameKind::Klondike | GameKind::Golf | GameKind::FortyEight => {
                    if tool_button(ui, text, Icon::Draw, "Piocher", "Piocher (Tab)", g && self.game.can_deal(), false).clicked() {
                        self.deal();
                    }
                }
                GameKind::Grandfather => {
                    if tool_button(ui, text, Icon::Deal, "Redistribuer", "Redistribuer (R)", g && self.game.can_deal(), false).clicked() {
                        self.deal();
                    }
                }
            }
            if self.game.uses_autodrop()
                && tool_button(ui, text, Icon::Drop, "Déposer", "Déposer sur les fondations (P)", g && self.can_drop_now(), self.dropping)
                    .clicked()
            {
                self.drop_to_foundations();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(e) = &self.error {
                ui.colored_label(Color32::LIGHT_RED, e);
            } else if self.selecting {
                ui.label("Choisissez un jeu.");
                return;
            } else {
                ui.label(self.solver.message());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let moves = self.game.moves();
                ui.label(format!("{moves} déplacement{}", if moves > 1 { "s" } else { "" }));
                ui.separator();
                ui.label(format!("Donne n° {}", self.game.deal_number()));
            });
        });
    }

    // ------------------------------------------------------------ dialogues

    fn dialogs(&mut self, ctx: &Context) {
        let mut close = false;
        let mut action = None;
        match &mut self.dialog {
            Dialog::None => {}
            Dialog::Numbered { variant, deal } => {
                egui::Window::new("Nouvelle donne numérotée")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        egui::Grid::new("numbered").num_columns(2).show(ui, |ui| {
                            ui.label("Jeu :");
                            variant_combo(ui, "numbered_variant", variant);
                            ui.end_row();
                            ui.label("Numéro de la donne :");
                            ui.add(egui::DragValue::new(deal).range(1..=i32::MAX as u32));
                            ui.end_row();
                        });
                        ui.horizontal(|ui| {
                            if ui.button("OK").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                action = Some(Action::Numbered(*variant, (*deal).max(1)));
                                close = true;
                            }
                            if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                                close = true;
                            }
                        });
                    });
            }
            Dialog::Stats { variant } => {
                let mut reset = None;
                egui::Window::new("Statistiques").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(
                    ctx,
                    |ui| {
                        variant_combo(ui, "stats_variant", variant);
                        ui.add_space(6.0);
                        let v = self.stats.get(*variant);
                        egui::Grid::new("stats").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                            ui.label("Parties jouées :");
                            ui.label(v.played.to_string());
                            ui.end_row();
                            ui.label("Parties gagnées :");
                            if v.played > 0 {
                                ui.label(format!("{} ({} %)", v.won, v.won * 100 / v.played));
                            } else {
                                ui.label(v.won.to_string());
                            }
                            ui.end_row();
                            ui.label("Plus longue série de victoires :");
                            ui.label(v.max_win_streak.to_string());
                            ui.end_row();
                            ui.label("Plus longue série de défaites :");
                            ui.label(v.max_lose_streak.to_string());
                            ui.end_row();
                            ui.label("Série actuelle :");
                            ui.label(if v.lose_streak > 0 {
                                plural(v.lose_streak, "défaite", "défaites")
                            } else {
                                plural(v.win_streak, "victoire", "victoires")
                            });
                            ui.end_row();
                            ui.label("Moins de déplacements :");
                            ui.label(v.min_moves.map_or("∞".to_string(), |m| m.to_string()));
                            ui.end_row();
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Réinitialiser").clicked() {
                                reset = Some(*variant);
                            }
                            if ui.button("Fermer").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                                close = true;
                            }
                        });
                    },
                );
                if let Some(v) = reset {
                    self.stats.reset(v);
                }
            }
            Dialog::Confirm { action: pending, dont_ask } => {
                egui::Window::new("Abandonner la partie en cours ?")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label("Une nouvelle partie est demandée, mais une partie est déjà en cours.");
                        ui.label("Si vous l'abandonnez, une défaite sera comptée dans les statistiques.");
                        ui.add_space(6.0);
                        ui.checkbox(dont_ask, "Ne plus demander");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Abandonner la partie").clicked() {
                                action = Some(*pending);
                                close = true;
                            }
                            if ui.button("Annuler").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                                close = true;
                            }
                        });
                    });
                if action.is_some() && *dont_ask {
                    self.settings.confirm_abandon = false;
                    self.settings.save();
                }
            }
            Dialog::About => {
                egui::Window::new(format!("À propos de {APP_NAME}"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.heading(format!("{APP_NAME} {}", env!("CARGO_PKG_VERSION")));
                        ui.label("Le Spider (1 à 4 couleurs), le FreeCell (et ses variantes), le Klondike (pioche 1 ou 3),\nl'horloge du grand-père, Simon le simple, le Golf, le Yukon, Grand-père, Mod3, les As, Quarante & Huit,\nTreize à la douzaine, Château et Normandie.");
                        ui.add_space(6.0);
                        ui.label("Portage en Rust de jeux de KPatience, le jeu de patience du projet KDE.");
                        ui.hyperlink_to("KPatience", "https://apps.kde.org/kpat");
                        ui.label("Spider de KPatience : Josh Metzler ; outil de résolution du Spider : Stephan Kulow ;");
                        ui.label("FreeCell et Klondike de KPatience : Rodolfo Borges, Paul Olav Tvete, Stephan Kulow, Parker Coates ;");
                        ui.label("outils de résolution du Klondike, de l'horloge, du Yukon, de Grand-père et de Mod3 (patsolve) : Tom Holroyd, Stephan Kulow ;");
                        ui.label("© 1995 Paul Olav Tvete, © 2000 Stephan Kulow et les auteurs de KPatience.");
                        ui.label("Jeux de cartes et thèmes : libkdegames et KPatience.");
                        ui.add_space(6.0);
                        ui.label("Licence : GNU GPL version 2 ou ultérieure.");
                        ui.add_space(6.0);
                        if ui.button("Fermer").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                            close = true;
                        }
                    });
            }
            Dialog::Rules => {
                let (title, text) = match self.game.kind() {
                    GameKind::Spider => ("Règles du Spider", RULES_SPIDER),
                    GameKind::FreeCell => ("Règles du FreeCell", RULES_FREECELL),
                    GameKind::Klondike => ("Règles du Klondike", RULES_KLONDIKE),
                    GameKind::Clock => ("Règles de l'horloge du grand-père", RULES_CLOCK),
                    GameKind::Simon => ("Règles de Simon le simple", RULES_SIMON),
                    GameKind::Golf => ("Règles du Golf", RULES_GOLF),
                    GameKind::Yukon => ("Règles du Yukon", RULES_YUKON),
                    GameKind::Grandfather => ("Règles de Grand-père", RULES_GRANDFATHER),
                    GameKind::Mod3 => ("Règles de Mod3", RULES_MOD3),
                    GameKind::AcesUp => ("Règles des As", RULES_ACES_UP),
                    GameKind::FortyEight => ("Règles de Quarante & Huit", RULES_FORTY_EIGHT),
                    GameKind::Bakers => ("Règles de Treize à la douzaine", RULES_BAKERS),
                    GameKind::Castle => ("Règles de Château", RULES_CASTLE),
                    GameKind::Normandy => ("Règles de Normandie", RULES_NORMANDY),
                };
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .default_width(460.0)
                    .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.label(text);
                        ui.add_space(6.0);
                        if ui.button("Fermer").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                            close = true;
                        }
                    });
            }
        }
        if close {
            self.dialog = Dialog::None;
        }
        if let Some(a) = action {
            self.perform(a);
        }
    }

    // ------------------------------------------------------------ tapis

    fn board(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let now = self.now();
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        let layout = Layout::new(rect, self.ratio, &self.game);

        // --- images (rendues en arrière-plan à la bonne taille)
        let card_px = ((layout.card.x * ppp).round() as u32, (layout.card.y * ppp).round() as u32);
        let mut names: Vec<(String, (u32, u32))> = Vec::with_capacity(53);
        names.push(("back".into(), card_px));
        for suit in cards::Suit::ALL {
            for rank in 1..=13 {
                names.push((cards::svg_face_name(rank, suit), card_px));
            }
        }
        let mut busy = self.cards.request(&ctx, &self.deck, names, card_px);

        let bg_px = ((rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32);
        let frame = self.message_frame_size(rect);
        let frame_px = ((frame.x * ppp).round() as u32, (frame.y * ppp).round() as u32);
        let decor_names = vec![("background".to_string(), bg_px), ("pile".to_string(), card_px), ("message_frame".to_string(), frame_px)];
        busy |= self.decor.request(&ctx, &self.theme, decor_names, (bg_px.0, bg_px.1 ^ card_px.0.rotate_left(16) ^ frame_px.0));

        // --- fond et emplacements
        match self.decor.textures.get("background") {
            Some(tex) => {
                painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_filled(rect, 0.0, Color32::from_rgb(0x1e, 0x5a, 0x2c));
            }
        }
        if self.selecting {
            busy |= self.selection_screen(&ctx, &painter, &response, rect, dt);
            if busy {
                ctx.request_repaint();
            }
            return;
        }
        for p in 0..layout.piles.len() {
            self.draw_slot(&painter, layout.slot_rect(p), self.highlights.contains(&Highlight::EmptyPile(p)));
        }

        // --- positions cibles et animation
        let placed = place_cards(&self.game, &layout);
        let k = 1.0 - (-dt * 14.0).exp();
        let mut animating = false;
        let dragging: Option<(usize, usize, Pos2)> = self
            .drag
            .as_ref()
            .and_then(|d| response.interact_pointer_pos().or(ctx.pointer_latest_pos()).map(|p| (d.from, d.index, p - d.grab)));
        let mut draw: Vec<(i32, Card, Pos2)> = Vec::with_capacity(placed.len());
        for p in &placed {
            let dragged = matches!((dragging, p.place), (Some((from, index, _)), Place::Pile(c, i)) if c == from && i >= index);
            let target = match dragging {
                Some((from, index, origin)) if dragged => origin + (p.pos - layout.card_pos(&self.game, from, index)),
                _ => p.pos,
            };
            let cur = self.shown.entry(p.card.id).or_insert(layout.deal_origin);
            if dragged {
                *cur = target;
            } else {
                let d = target - *cur;
                if d.length() < 0.5 {
                    *cur = target;
                } else {
                    *cur += d * k;
                    animating = true;
                }
            }
            let mut z = p.z;
            if dragged {
                z += 2000;
            } else if (target - *cur).length() > 2.0 {
                z += 1000;
            }
            draw.push((z, p.card, *cur));
        }
        draw.sort_by_key(|(z, _, _)| *z);

        // --- cartes
        for (_, card, pos) in &draw {
            let r = Rect::from_min_size(*pos, layout.card);
            self.draw_card(&painter, r, card);
            if self.highlights.contains(&Highlight::Card(card.id)) {
                painter.rect_filled(r, 0.06 * layout.card.x, Color32::from_rgba_unmultiplied(70, 130, 255, 80));
                painter.rect_stroke(r, 0.06 * layout.card.x, Stroke::new(2.5, Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Inside);
            }
        }

        // --- cible du glisser-déposer
        if let Some((from, index, origin)) = dragging {
            if let Some(to) = self.drop_target(&layout, from, index, origin) {
                let r = match self.game.pile(to).len() {
                    0 => layout.slot_rect(to),
                    n => Rect::from_min_size(layout.card_pos(&self.game, to, n - 1), layout.card),
                };
                painter.rect_stroke(r, 0.06 * layout.card.x, Stroke::new(3.0, Color32::from_rgb(255, 215, 90)), egui::StrokeKind::Inside);
            }
        }

        // --- interactions
        self.interact(&response, &layout, &draw);

        // --- victoire
        if self.game.is_won() && !animating {
            self.draw_won(&painter, rect);
        }

        // --- démonstration, dépôt automatique, outil de résolution
        if self.solver.poll(&self.game, now) && self.hint_waiting {
            self.show_hint();
        }
        let moving = animating || self.drag.is_some();
        self.step_demo(now, moving);
        self.step_autodrop(moving || !matches!(self.dialog, Dialog::None));

        if animating || busy || self.demo || self.dropping || self.solver.calculating() || self.drag.is_some() {
            ctx.request_repaint();
        }
    }

    fn message_frame_size(&self, rect: Rect) -> Vec2 {
        let aspect = self.theme.element_size("message_frame").map_or(3.0, |(w, h)| w / h);
        let w = (rect.width() * 0.5).min(rect.height() * 0.4 * aspect);
        Vec2::new(w, w / aspect)
    }

    fn draw_slot(&self, painter: &egui::Painter, r: Rect, highlighted: bool) {
        match self.decor.textures.get("pile") {
            Some(tex) => {
                painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_stroke(r, 0.06 * r.width(), Stroke::new(1.5, Color32::from_white_alpha(60)), egui::StrokeKind::Inside);
            }
        }
        if highlighted {
            painter.rect_filled(r, 0.06 * r.width(), Color32::from_rgba_unmultiplied(70, 130, 255, 80));
            painter.rect_stroke(r, 0.06 * r.width(), Stroke::new(2.5, Color32::from_rgb(120, 180, 255)), egui::StrokeKind::Inside);
        }
    }

    fn draw_card(&self, painter: &egui::Painter, r: Rect, card: &Card) {
        let name = if card.face_up { card.svg_name() } else { "back".to_string() };
        match self.cards.textures.get(&name) {
            Some(tex) => {
                painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                // Images pas encore prêtes : une carte simple.
                let rounding = 0.06 * r.width();
                if card.face_up {
                    painter.rect_filled(r, rounding, Color32::from_rgb(250, 250, 245));
                    let text = format!("{}{}", rank_label(card.rank), suit_symbol(card.suit));
                    painter.text(
                        r.left_top() + Vec2::new(0.08 * r.width(), 0.05 * r.width()),
                        Align2::LEFT_TOP,
                        text,
                        FontId::proportional(0.28 * r.width()),
                        if card.suit.is_red() { Color32::from_rgb(200, 20, 30) } else { Color32::BLACK },
                    );
                } else {
                    painter.rect_filled(r, rounding, Color32::from_rgb(40, 70, 140));
                }
                painter.rect_stroke(r, rounding, Stroke::new(1.0, Color32::from_gray(60)), egui::StrokeKind::Inside);
            }
        }
    }

    fn draw_won(&self, painter: &egui::Painter, rect: Rect) {
        let size = self.message_frame_size(rect);
        let r = Rect::from_center_size(rect.center(), size);
        match self.decor.textures.get("message_frame") {
            Some(tex) => {
                painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_filled(r, 12.0, Color32::from_rgba_unmultiplied(240, 240, 230, 230));
            }
        }
        let text =
            if self.help_used { "Félicitations ! La partie a été gagnée ensemble." } else { "Félicitations ! Vous avez gagné." };
        // Taille de police : 80 % du cadre au plus.
        let mut size_pt = 36.0;
        let galley = loop {
            let g = painter.layout_no_wrap(text.to_string(), FontId::proportional(size_pt), Color32::BLACK);
            if (g.size().x <= r.width() * 0.8 && g.size().y <= r.height() * 0.8) || size_pt <= 8.0 {
                break g;
            }
            size_pt -= 1.0;
        };
        painter.galley(r.center() - galley.size() / 2.0, galley, Color32::BLACK);
    }

    /// La pile où lâcher les cartes tirées (meilleur recouvrement), si le coup est permis.
    fn drop_target(&self, layout: &Layout, from: usize, index: usize, origin: Pos2) -> Option<usize> {
        let dragged = Rect::from_min_size(origin, layout.card);
        (0..self.game.pile_count())
            .filter(|&to| to != from && self.game.can_move(from, index, to))
            .map(|to| (to, dragged.intersect(layout.piles[to].zone).area()))
            .filter(|(_, a)| *a > 0.0)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(to, _)| to)
    }

    fn hit(&self, draw: &[(i32, Card, Pos2)], layout: &Layout, p: Pos2) -> Option<Place> {
        let card_id = draw.iter().rev().find(|(_, _, pos)| Rect::from_min_size(*pos, layout.card).contains(p)).map(|(_, c, _)| c.id)?;
        for pile in 0..self.game.pile_count() {
            if let Some(i) = self.game.pile(pile).iter().position(|c| c.id == card_id) {
                return Some(Place::Pile(pile, i));
            }
        }
        if let AnyGame::Spider(g) = &self.game {
            for (r, row) in g.stock.iter().enumerate() {
                if row.iter().any(|c| c.id == card_id) {
                    return Some(Place::Stock(r));
                }
            }
        }
        None
    }

    fn interact(&mut self, response: &egui::Response, layout: &Layout, draw: &[(i32, Card, Pos2)]) {
        if self.game.is_won() || !matches!(self.dialog, Dialog::None) {
            self.drag = None;
            return;
        }
        if response.drag_started() {
            let press = response.ctx.input(|i| i.pointer.press_origin());
            if let Some(p) = press {
                if let Some(Place::Pile(pile, i)) = self.hit(draw, layout, p) {
                    if self.game.can_pick(pile, i) {
                        self.demo = false;
                        self.dropping = false;
                        let origin = self.shown.get(&self.game.pile(pile)[i].id).copied().unwrap_or(p);
                        self.drag = Some(Drag { from: pile, index: i, grab: p - origin });
                    }
                }
            }
        }
        if response.drag_stopped() {
            if let Some(d) = self.drag.take() {
                let pointer = response.interact_pointer_pos().or(response.ctx.pointer_latest_pos());
                if let Some(p) = pointer {
                    if let Some(to) = self.drop_target(layout, d.from, d.index, p - d.grab) {
                        self.play(Move::Cards { from: d.from, index: d.index, to });
                    }
                }
            }
            return;
        }
        // Klondike, Golf, Mod3, les As, Quarante & Huit et Normandie : un clic sur le talon (ou sa place vide) pioche,
        // même en double-clic.
        let on_klondike_stock = |p: Pos2| self.game.stock_pile().is_some_and(|sp| layout.slot_rect(sp).contains(p));
        if response.clicked() || response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos()
                && on_klondike_stock(p)
            {
                self.deal();
                return;
            }
        }
        if response.double_clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                if let Some(Place::Pile(pile, i)) = self.hit(draw, layout, p) {
                    self.auto_move(pile, i);
                }
            }
        } else if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                let hit = self.hit(draw, layout, p);
                // Golf et les As : un clic joue la carte (`tryAutomaticMove`).
                if let (AnyGame::Golf(_) | AnyGame::AcesUp(_), Some(Place::Pile(pile, i))) = (&self.game, hit)
                    && let Some(m) = self.game.auto_move(pile, i)
                {
                    self.play(m);
                    return;
                }
                let on_stock = matches!(hit, Some(Place::Stock(_)));
                if on_stock || (layout.stock_rect().is_some_and(|r| r.contains(p)) && self.game.can_deal()) {
                    self.deal();
                } else if self.hint_active() {
                    self.clear_hint();
                }
            }
        }
    }

    fn update_title(&mut self, ctx: &Context) {
        let title = if self.selecting {
            APP_NAME.to_string()
        } else {
            format!("{} - {} — {APP_NAME}", self.game.title(), self.game.deal_number())
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn save_on_exit(&mut self) {
        if self.selecting {
            storage::clear_state();
        } else if self.settings.remember_state && !self.game.is_won() {
            storage::save_state(&SavedState { game: self.game.clone(), help_used: self.help_used });
        } else {
            self.record_if_in_progress();
            storage::clear_state();
        }
        self.settings.save();
    }
}

impl eframe::App for SpiderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.update_title(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        if self.settings.show_toolbar {
            egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        }
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.board(ui));
        self.dialogs(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_on_exit();
    }
}

/// Liste déroulante des variantes (Spider 1 à 4 couleurs, variantes du FreeCell).
fn variant_combo(ui: &mut egui::Ui, id: &str, variant: &mut Variant) {
    egui::ComboBox::from_id_salt(id).width(220.0).selected_text(variant.name()).show_ui(ui, |ui| {
        for v in Variant::ALL {
            ui.selectable_value(variant, v, v.name());
        }
    });
}

// =================================================================== choix du jeu

/// Les jeux de l'écran de choix, dans l'ordre alphabétique (comme
/// KPatience), avec leur aperçu (`previews/` de KPatience).
const GAMES: [(GameKind, &str, &[u8]); 14] = [
    (GameKind::Castle, "Château", include_bytes!("../data/previews/castle.png")),
    (GameKind::FreeCell, "FreeCell", include_bytes!("../data/previews/freecell.png")),
    (GameKind::Golf, "Golf", include_bytes!("../data/previews/golf.png")),
    (GameKind::Grandfather, "Grand-père", include_bytes!("../data/previews/grandf.png")),
    (GameKind::Klondike, "Klondike", include_bytes!("../data/previews/klondike.png")),
    (GameKind::AcesUp, "Les as", include_bytes!("../data/previews/aces.png")),
    (GameKind::Clock, "L'horloge du grand-père", include_bytes!("../data/previews/clock.png")),
    (GameKind::Mod3, "Mod3", include_bytes!("../data/previews/mod3.png")),
    (GameKind::Normandy, "Normandie", include_bytes!("../data/previews/normandy.png")),
    (GameKind::FortyEight, "Quarante & Huit", include_bytes!("../data/previews/fortyeight.png")),
    (GameKind::Simon, "Simon le simple", include_bytes!("../data/previews/simon.png")),
    (GameKind::Spider, "Spider", include_bytes!("../data/previews/spider.png")),
    (GameKind::Bakers, "Treize à la douzaine", include_bytes!("../data/previews/bakers.png")),
    (GameKind::Yukon, "Yukon", include_bytes!("../data/previews/yukon.png")),
];
/// Constantes de `GameSelectionScene`.
const SELECT_SPACING: f32 = 0.10;
const SELECT_TEXT_HEIGHT: f32 = 1.0 / 6.0;
const SELECT_TEXT_WIDTH: f32 = 0.57;
const SELECT_PADDING: f32 = 0.037;
const SELECT_FADE: f32 = 0.3;

struct Selection {
    /// La bulle surlignée (pointeur ou clavier).
    index: Option<usize>,
    /// La bulle sous le pointeur à l'image précédente.
    hovered: Option<usize>,
    /// Avancement du fondu vers la bulle surlignée (0 à 1), par jeu.
    fade: [f32; GAMES.len()],
    bubbles: Rendered,
    previews: Vec<TextureHandle>,
    /// Couleurs du texte, normal et surligné (`bubble_text_color`,
    /// `bubble_hover_text_color` du thème).
    text: (Color32, Color32),
    columns: usize,
}

impl Selection {
    fn new(theme: &Svg) -> Selection {
        // Comme KPatience (`Renderer::colorOfElement`) : noir si le thème
        // ne définit pas la couleur (Royal Hearts).
        let dark = Color32::BLACK;
        Selection {
            index: None,
            hovered: None,
            fade: [0.0; GAMES.len()],
            bubbles: Rendered::new(),
            previews: Vec::new(),
            text: (
                theme.element_color("bubble_text_color").unwrap_or(dark),
                theme.element_color("bubble_hover_text_color").unwrap_or(dark),
            ),
            columns: 1,
        }
    }
}

fn mix_colors(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Une image ajustée (sans déformation) dans `area`, centrée.
fn fit(size: Vec2, area: Rect) -> Rect {
    let k = (area.width() / size.x).min(area.height() / size.y);
    Rect::from_center_size(area.center(), size * k)
}

impl SpiderApp {
    /// La grille des bulles (`GameSelectionScene::resizeScene`) : le nombre
    /// de lignes qui donne à la grille la forme la plus proche de la fenêtre,
    /// puis les plus grandes bulles qui tiennent, centrées.
    fn selection_boxes(&mut self, rect: Rect) -> Vec<Rect> {
        let n = GAMES.len();
        let box_aspect = self.theme.element_size("bubble").map_or(1.5, |(w, h)| w / h);
        let scene_aspect = rect.width() / rect.height();
        let mut best = (f32::MAX, 1);
        for rows in 1..=n {
            let columns = n.div_ceil(rows);
            let non_empty = n.div_ceil(columns);
            let grid = box_aspect * columns as f32 / non_empty as f32;
            let error = if grid > scene_aspect { grid / scene_aspect } else { scene_aspect / grid };
            if error < best.0 {
                best = (error, rows);
            }
        }
        let rows = best.1;
        let columns = n.div_ceil(rows);
        self.selection.columns = columns;
        let (c, r) = (columns as f32, rows as f32);
        let sp = SELECT_SPACING;
        let grid_aspect = box_aspect * (c + sp * (c + 1.0)) / (r + sp * (r + 1.0));
        let (bw, bh) = if grid_aspect > scene_aspect {
            let bw = rect.width() / (c + sp * (c + 1.0));
            (bw, bw / box_aspect)
        } else {
            let bh = rect.height() / (r + sp * (r + 1.0));
            (bh * box_aspect, bh)
        };
        let grid = Vec2::new(bw * (c + sp * (c + 1.0)), bh * (r + sp * (r + 1.0)));
        let origin = rect.min + (rect.size() - grid) / 2.0 + Vec2::new(bw * sp, bh * sp);
        (0..n)
            .map(|i| {
                let (col, row) = ((i % columns) as f32, (i / columns) as f32);
                Rect::from_min_size(origin + Vec2::new(col * bw * (1.0 + sp), row * bh * (1.0 + sp)), Vec2::new(bw, bh))
            })
            .collect()
    }

    /// Dessine l'écran de choix et gère la souris. Renvoie vrai s'il faut
    /// redessiner (fondu, images en cours de rendu).
    fn selection_screen(&mut self, ctx: &Context, painter: &egui::Painter, response: &egui::Response, rect: Rect, dt: f32) -> bool {
        let boxes = self.selection_boxes(rect);
        let Some(first) = boxes.first() else { return false };
        let (bw, bh) = (first.width(), first.height());
        let ppp = ctx.pixels_per_point();
        let px = ((bw * ppp).round() as u32, (bh * ppp).round() as u32);
        let names = vec![("bubble".to_string(), px), ("bubble_hover".to_string(), px)];
        let mut busy = self.selection.bubbles.request(ctx, &self.theme, names, px);
        if self.selection.previews.is_empty() {
            for (kind, _, png) in GAMES {
                let image = assets::decode_png(png).unwrap_or_else(|| ColorImage::new([1, 1], vec![Color32::TRANSPARENT]));
                self.selection.previews.push(ctx.load_texture(format!("preview-{kind:?}"), image, TextureOptions::LINEAR));
            }
        }

        // Survol : la bulle sous le pointeur devient la bulle surlignée.
        let hovered = ctx.pointer_hover_pos().and_then(|p| boxes.iter().position(|b| b.contains(p)));
        if hovered != self.selection.hovered {
            match hovered {
                Some(h) => self.selection.index = Some(h),
                None if self.selection.index == self.selection.hovered => self.selection.index = None,
                None => {}
            }
            self.selection.hovered = hovered;
        }
        for (i, f) in self.selection.fade.iter_mut().enumerate() {
            let target = if self.selection.index == Some(i) { 1.0 } else { 0.0 };
            if *f != target {
                let step = dt / SELECT_FADE;
                *f = if target > *f { (*f + step).min(1.0) } else { (*f - step).max(0.0) };
                busy = true;
            }
        }

        // Taille du texte : la même pour toutes les bulles, la plus grande
        // qui fait tenir chaque nom dans 57 % de la largeur.
        let mut font = (bh * (SELECT_TEXT_HEIGHT - 1.5 * SELECT_PADDING)).max(5.0);
        let label_width = |size: f32| {
            GAMES
                .iter()
                .map(|(_, name, _)| painter.layout_no_wrap(name.to_string(), FontId::proportional(size), Color32::BLACK).size().x)
                .fold(0.0, f32::max)
        };
        while font > 5.0 && label_width(font) > bw * SELECT_TEXT_WIDTH {
            font -= 1.0;
        }

        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for (i, b) in boxes.iter().enumerate() {
            let fade = self.selection.fade[i];
            match self.selection.bubbles.textures.get("bubble") {
                Some(tex) if fade < 1.0 => {
                    painter.image(tex.id(), *b, uv, Color32::WHITE);
                }
                Some(_) => {}
                None => {
                    painter.rect_filled(*b, 0.04 * bw, Color32::from_rgba_unmultiplied(20, 20, 40, 160));
                }
            }
            if fade > 0.0 {
                match self.selection.bubbles.textures.get("bubble_hover") {
                    Some(tex) => {
                        painter.image(tex.id(), *b, uv, Color32::WHITE.gamma_multiply(fade));
                    }
                    None => {
                        painter.rect_filled(*b, 0.04 * bw, Color32::from_white_alpha((fade * 60.0) as u8));
                    }
                }
            }
            let pad = SELECT_PADDING * bw;
            let text_h = bh * SELECT_TEXT_HEIGHT;
            // Comme KPatience : l'aperçu tient dans un rectangle aussi large
            // que la bulle est haute (moins les marges), centré sous le titre.
            let area = Rect::from_min_size(
                Pos2::new(b.center().x - (bh - 2.0 * pad) / 2.0, b.min.y + pad + text_h),
                Vec2::new(bh - 2.0 * pad, bh - 2.0 * pad - text_h),
            );
            let tex = &self.selection.previews[i];
            painter.image(tex.id(), fit(tex.size_vec2(), area), uv, Color32::WHITE);
            let text_rect = Rect::from_min_size(b.min, Vec2::new(bw, text_h));
            let color = mix_colors(self.selection.text.0, self.selection.text.1, fade);
            painter.text(text_rect.center(), Align2::CENTER_CENTER, GAMES[i].1, FontId::proportional(font), color);
        }

        if response.clicked() && matches!(self.dialog, Dialog::None) {
            if let Some(i) = response.interact_pointer_pos().and_then(|p| boxes.iter().position(|b| b.contains(p))) {
                self.game_selected(GAMES[i].0);
            }
        }
        busy
    }

    /// Clavier sur l'écran de choix (`GameSelectionScene::keyReleaseEvent`) :
    /// une première flèche surligne la première bulle, puis les flèches se
    /// déplacent dans la grille ; Entrée ou Espace choisit.
    fn selection_keys(&mut self, ctx: &Context) {
        let n = GAMES.len();
        let columns = self.selection.columns.max(1);
        let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let (up, down, left, right) = (key(Key::ArrowUp), key(Key::ArrowDown), key(Key::ArrowLeft), key(Key::ArrowRight));
        let choose = key(Key::Enter) || key(Key::Space);
        match self.selection.index {
            None if up || down || left || right => self.selection.index = Some(0),
            None => {}
            Some(i) => {
                let j = if up && i / columns > 0 {
                    i - columns
                } else if down && i + columns < n {
                    i + columns
                } else if left && i % columns > 0 {
                    i - 1
                } else if right && i % columns < columns - 1 && i + 1 < n {
                    i + 1
                } else {
                    i
                };
                self.selection.index = Some(j);
                if choose {
                    self.game_selected(GAMES[j].0);
                }
            }
        }
    }
}

// =================================================================== barre d'outils

#[derive(Clone, Copy)]
enum Icon {
    New,
    Restart,
    Undo,
    Redo,
    Hint,
    Demo,
    Pause,
    Deal,
    Drop,
    Shuffle,
    Draw,
}

/// Bouton de barre d'outils : une icône et son texte à côté (comme dans KDE).
/// `show_text` : le texte à côté de l'icône ; sinon, l'icône seule (le texte reste dans l'infobulle).
fn tool_button(
    ui: &mut egui::Ui,
    show_text: bool,
    icon: Icon,
    label: &str,
    tooltip: &str,
    enabled: bool,
    selected: bool,
) -> egui::Response {
    const ICON: f32 = 18.0;
    const GAP: f32 = 5.0;
    let pad = Vec2::new(6.0, 4.0);
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::PLACEHOLDER);
    let inner_h = ICON.max(galley.size().y);
    let text_w = if show_text { GAP + galley.size().x } else { 0.0 };
    let size = Vec2::new(2.0 * pad.x + ICON + text_w, 2.0 * pad.y + inner_h);
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let visuals = ui.style().interact_selectable(&response, selected);
        if enabled && (selected || response.hovered()) {
            painter.rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
        }
        let color = if enabled { visuals.text_color() } else { ui.visuals().weak_text_color().gamma_multiply(0.6) };
        let icon_rect = Rect::from_min_size(rect.min + pad + Vec2::new(0.0, (inner_h - ICON) / 2.0), Vec2::splat(ICON));
        draw_icon(painter, icon_rect, icon, color, ui.visuals().panel_fill);
        let text_pos = rect.min + pad + Vec2::new(ICON + GAP, (inner_h - galley.size().y) / 2.0);
        if show_text {
            painter.galley(text_pos, galley, color);
        }
    }
    response.on_hover_text(tooltip)
}

/// Points d'un arc de cercle (angles en radians, sens des aiguilles d'une montre à l'écran).
fn arc_points(center: Pos2, radius: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = 24;
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            center + radius * Vec2::angled(a)
        })
        .collect()
}

/// Pointe de flèche pleine en `tip`, dans la direction `dir`.
fn arrow_head(painter: &egui::Painter, tip: Pos2, dir: Vec2, size: f32, color: Color32) {
    let d = dir.normalized();
    let n = d.rot90();
    let base = tip - d * size;
    painter.add(egui::Shape::convex_polygon(vec![tip, base + n * size * 0.6, base - n * size * 0.6], color, Stroke::NONE));
}

/// Icônes dessinées en vecteur, dans un carré `r`.
/// `bg` : couleur du fond, pour séparer les cartes de l'icône « Distribuer ».
fn draw_icon(painter: &egui::Painter, r: Rect, icon: Icon, color: Color32, bg: Color32) {
    let s = r.width();
    let p = |x: f32, y: f32| r.min + Vec2::new(x * s, y * s);
    let stroke = Stroke::new((s * 0.1).max(1.5), color);
    let card = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(p(x0, y0), p(x1, y1));
    match icon {
        Icon::New => {
            painter.rect_stroke(card(0.12, 0.08, 0.62, 0.86), 0.08 * s, stroke, egui::StrokeKind::Middle);
            painter.line_segment([p(0.78, 0.48), p(0.78, 0.92)], stroke);
            painter.line_segment([p(0.56, 0.70), p(1.0, 0.70)], stroke);
        }
        Icon::Restart => {
            let c = p(0.5, 0.5);
            let (from, to) = (-1.2, 4.3);
            let pts = arc_points(c, 0.34 * s, from, to);
            let end = *pts.last().unwrap();
            painter.line(pts, stroke);
            // Pointe dans le prolongement de l'arc (tangente au bout).
            let tangent = Vec2::angled(to + std::f32::consts::FRAC_PI_2);
            arrow_head(painter, end + tangent * 0.12 * s, tangent, 0.26 * s, color);
        }
        Icon::Undo | Icon::Redo => {
            // Dessinée pour « Annuler », retournée pour « Refaire ».
            let m = |x: f32, y: f32| if matches!(icon, Icon::Undo) { p(x, y) } else { p(1.0 - x, y) };
            let flip = if matches!(icon, Icon::Undo) { 1.0 } else { -1.0 };
            let center = m(0.58, 0.57);
            let mut pts = vec![m(0.3, 0.32)];
            pts.extend(
                arc_points(Pos2::ZERO, 0.25 * s, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2)
                    .into_iter()
                    .map(|q| center + Vec2::new(q.x * flip, q.y)),
            );
            pts.push(m(0.32, 0.82));
            painter.line(pts, stroke);
            arrow_head(painter, m(0.04, 0.32), Vec2::new(-flip, 0.0), 0.3 * s, color);
        }
        Icon::Hint => {
            painter.circle_stroke(p(0.5, 0.38), 0.28 * s, stroke);
            painter.line_segment([p(0.36, 0.76), p(0.64, 0.76)], stroke);
            painter.line_segment([p(0.40, 0.92), p(0.60, 0.92)], stroke);
            painter.line_segment([p(0.5, 0.38), p(0.5, 0.62)], Stroke::new(stroke.width * 0.8, color));
        }
        Icon::Demo => {
            painter.add(egui::Shape::convex_polygon(vec![p(0.2, 0.1), p(0.88, 0.5), p(0.2, 0.9)], color, Stroke::NONE));
        }
        Icon::Pause => {
            painter.rect_filled(card(0.2, 0.12, 0.42, 0.88), 0.03 * s, color);
            painter.rect_filled(card(0.58, 0.12, 0.8, 0.88), 0.03 * s, color);
        }
        Icon::Draw => {
            // Le talon, et une carte retournée à côté.
            let back = card(0.04, 0.16, 0.5, 0.84);
            painter.rect_filled(back, 0.07 * s, color);
            let front = card(0.5, 0.16, 0.96, 0.84);
            painter.rect_stroke(front, 0.07 * s, stroke, egui::StrokeKind::Inside);
        }
        Icon::Shuffle => {
            // Deux flèches qui se croisent.
            let top = [p(0.06, 0.28), p(0.32, 0.28), p(0.62, 0.72), p(0.82, 0.72)];
            let bottom = [p(0.06, 0.72), p(0.32, 0.72), p(0.62, 0.28), p(0.82, 0.28)];
            painter.line(top.to_vec(), stroke);
            painter.line(bottom.to_vec(), stroke);
            arrow_head(painter, p(1.0, 0.72), Vec2::new(1.0, 0.0), 0.22 * s, color);
            arrow_head(painter, p(1.0, 0.28), Vec2::new(1.0, 0.0), 0.22 * s, color);
        }
        Icon::Drop => {
            // Une carte, et une flèche qui descend dessus.
            painter.rect_stroke(card(0.22, 0.46, 0.78, 0.96), 0.07 * s, stroke, egui::StrokeKind::Middle);
            painter.line_segment([p(0.5, 0.02), p(0.5, 0.26)], stroke);
            arrow_head(painter, p(0.5, 0.42), Vec2::new(0.0, 1.0), 0.2 * s, color);
        }
        Icon::Deal => {
            // La réserve : trois tas qui se chevauchent.
            for x in [0.0, 0.2, 0.4] {
                let c = card(x + 0.02, 0.14, x + 0.56, 0.86);
                painter.rect_filled(c, 0.07 * s, color);
                painter.rect_stroke(c, 0.07 * s, Stroke::new((s * 0.06).max(1.0), bg), egui::StrokeKind::Inside);
            }
        }
    }
}

// =================================================================== utilitaires

fn random_deal() -> u32 {
    fastrand::u32(1..i32::MAX as u32)
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

fn rank_label(rank: u8) -> String {
    match rank {
        1 => "A".into(),
        11 => "V".into(),
        12 => "D".into(),
        13 => "R".into(),
        r => r.to_string(),
    }
}

fn suit_symbol(suit: cards::Suit) -> &'static str {
    match suit {
        cards::Suit::Clubs => "♣",
        cards::Suit::Diamonds => "♦",
        cards::Suit::Hearts => "♥",
        cards::Suit::Spades => "♠",
    }
}

const RULES_SPIDER: &str = "\
But : ranger les 104 cartes en 8 suites complètes, du Roi à l'As, de la même couleur. \
Une suite complète quitte toute seule le tapis.

• On peut déplacer une carte, ou une suite de cartes de la même couleur qui se suivent \
en descendant, sur une carte de rang immédiatement supérieur (de n'importe quelle couleur) \
ou sur une colonne vide.
• Quand la dernière carte d'une colonne est face cachée, elle se retourne.
• Un clic sur la réserve (en bas à droite) distribue une rangée : une carte sur chaque colonne.
• Double-cliquez sur une carte pour l'envoyer à la meilleure place.

Le jeu se joue avec 1, 2, 3 ou 4 couleurs : plus il y en a, plus c'est difficile.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant (la carte et sa destination) ; sinon, toutes les cartes qui peuvent être déplacées.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_FREECELL: &str = "\
But : monter toutes les cartes sur les fondations (en haut à droite), de l'As au Roi, \
une fondation par couleur.

• Toutes les cartes sont visibles. Dans les colonnes, on pose une carte sur une carte de \
rang immédiatement supérieur et de couleur opposée (rouge sur noir, noir sur rouge).
• Chaque cellule libre (en haut à gauche) garde une carte, n'importe laquelle.
• Une colonne vide reçoit n'importe quelle carte.
• On déplace une seule carte à la fois ; plusieurs cartes qui se suivent peuvent partir \
ensemble s'il y a assez de cellules libres et de colonnes vides pour les déplacer une à \
une : (cellules libres + 1) × 2 par colonne vide.
• Double-cliquez sur une carte pour l'envoyer sur une fondation, sinon dans une cellule.
• Les cartes qui ne servent plus montent seules sur les fondations (dépôt automatique, \
qu'on peut couper dans la configuration ; « Déposer » (P) le fait alors à la demande).

Variantes :
• Baker's Game : les suites se font dans la même couleur.
• Eight Off : 8 cellules, dont 4 remplies au départ ; même couleur ; Rois seulement \
dans les colonnes vides.
• Forecell : 4 cellules remplies au départ ; Rois seulement dans les colonnes vides.
• Seahaven Towers : 10 colonnes, 2 cellules remplies au départ ; même couleur ; Rois \
seulement dans les colonnes vides.
La configuration permet aussi de choisir chaque règle (variante personnalisée), le \
nombre de cellules, de colonnes et de paquets.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, toutes les cartes qui peuvent être déplacées utilement.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_KLONDIKE: &str = "\
But : monter toutes les cartes sur les fondations (en haut à droite), de l'As au Roi, \
une fondation par couleur.

• Dans les colonnes, on pose une carte (ou une suite) sur une carte de rang immédiatement \
supérieur et de couleur opposée (rouge sur noir, noir sur rouge). Une colonne vide ne \
reçoit qu'un Roi. La carte découverte se retourne.
• Un clic sur le talon (en haut à gauche) retourne 1 ou 3 cartes sur la pioche, selon la \
variante choisie dans la configuration ; seule la carte du dessus de la pioche se joue. \
Quand le talon est vide, un clic y retourne la pioche, autant de fois qu'on veut.
• En pioche 1, on peut reprendre la carte du dessus d'une fondation.
• Double-cliquez sur une carte pour l'envoyer sur une fondation, sinon sur une colonne.
• Les cartes qui ne servent plus montent seules sur les fondations (dépôt automatique, \
qu'on peut couper dans la configuration ; « Déposer » (P) le fait alors à la demande).

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les cartes qui peuvent être déplacées utilement.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_CLOCK: &str = "\
But : vider les 8 piles de gauche en montant toutes les cartes sur le cadran.

• Le cadran a 12 fondations, une par heure, qui commencent chacune par une carte \
déjà en place. Une fondation reçoit la carte suivante de sa couleur ; après le Roi \
vient l'As.
• Seule la carte du dessus d'une pile se déplace : sur une fondation, sur une carte de \
rang immédiatement supérieur (de n'importe quelle couleur), ou sur une pile vide.
• Double-cliquez sur une carte pour l'envoyer sur le cadran, sinon sur une autre pile.
• Toutes les cartes sont visibles : il n'y a pas de hasard une fois la donne faite.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les cartes qui peuvent être déplacées utilement.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_SIMON: &str = "\
But : monter les 4 couleurs sur les fondations (en haut), chacune en une suite \
complète du Roi à l'As.

• Toutes les cartes sont visibles. On pose une carte sur une carte de rang \
immédiatement supérieur, de n'importe quelle couleur, ou sur une colonne vide.
• On peut déplacer une suite de cartes qui se suivent en descendant ; si elle mêle \
plusieurs couleurs, il faut des colonnes vides pour l'appuyer : 2 tronçons de même \
couleur avec une colonne vide, 4 avec deux, 8 avec trois…
• Seule une suite complète du Roi à l'As d'une même couleur monte sur une fondation ; \
elle y monte toute seule (dépôt automatique, qu'on peut couper dans la configuration ; \
« Déposer » (P) le fait alors à la demande).
• Double-cliquez sur une carte pour envoyer sa suite à la meilleure place.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les cartes qui peuvent être déplacées utilement.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_GOLF: &str = "\
But : vider les 7 colonnes en posant toutes leurs cartes sur le rebut (en bas).

• Seule la carte du dessus d'une colonne se joue : elle va sur le rebut si son rang \
suit ou précède celui de la carte du dessus du rebut, quelle que soit sa couleur \
(un 7 sur un 6 ou sur un 8). On ne passe pas du Roi à l'As.
• Cliquez sur une carte pour l'envoyer sur le rebut.
• Un clic sur le talon (Tab) retourne sa carte du dessus sur le rebut. Le talon ne se \
refait pas.
• Quand les colonnes sont vides, le reste du talon se retourne tout seul : la partie est \
gagnée.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les cartes qui peuvent aller sur le rebut.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_YUKON: &str = "\
But : monter les 4 couleurs sur les fondations (à droite), de l'As au Roi.

• Il n'y a pas de talon : toutes les cartes sont dans les 7 colonnes, certaines face cachée.
• On pose une carte sur une carte de rang immédiatement supérieur et de l'autre teinte \
(rouge sur noir, noir sur rouge). Une colonne vide ne reçoit qu'un Roi.
• N'importe quelle carte visible se déplace, avec toutes les cartes qui sont dessus, \
même si elles ne se suivent pas. La carte découverte se retourne.
• Une carte montée sur une fondation n'en redescend pas. Les sorties sans risque se \
font toutes seules (dépôt automatique, qu'on peut couper dans la configuration ; \
« Déposer » (P) le fait alors à la demande).
• Double-cliquez sur une carte pour l'envoyer à la meilleure place.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_GRANDFATHER: &str = "\
But : monter les 4 couleurs sur les fondations (en haut), de l'As au Roi.

• Toutes les cartes sont dans les 7 colonnes, certaines face cachée.
• On pose une carte sur une carte de rang immédiatement supérieur de la même couleur \
(un 7 de cœur sur le 8 de cœur). Une colonne vide ne reçoit qu'un Roi.
• N'importe quelle carte visible se déplace, avec toutes les cartes qui sont dessus, \
même si elles ne se suivent pas. La carte découverte se retourne.
• Deux fois par partie, « Redistribuer » (R) ramasse les colonnes et les distribue de \
nouveau.
• Une carte montée sur une fondation n'en redescend pas. Les sorties se font toutes \
seules (dépôt automatique, qu'on peut couper dans la configuration ; « Déposer » (P) le \
fait alors à la demande).
• Double-cliquez sur une carte pour l'envoyer à la meilleure place.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_MOD3: &str = "\
But : ranger les 104 cartes (2 jeux) sur les 3 rangées de fondations et la pile des As.

• 1re rangée : des piles qui commencent par un 2 et montent de 3 en 3 dans la même \
couleur (2, 5, 8, Valet). 2e rangée : 3, 6, 9, Dame. 3e rangée : 4, 7, 10, Roi. Une pile \
mal commencée (par une carte distribuée au hasard) ne reçoit rien : il faut la libérer.
• La 4e rangée est la réserve : une pile vide y reçoit n'importe quelle carte ; vidée, \
elle reprend aussitôt la carte du dessus du talon.
• Seule la carte du dessus d'une pile se déplace. Les As vont sur leur pile à droite, \
tout seuls (dépôt automatique).
• « Distribuer » (Entrée, ou un clic sur le talon) pose une carte sur chaque pile de la \
réserve.
• Double-cliquez sur une carte pour l'envoyer sur une fondation qui la prend.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_ACES_UP: &str = "\
But : écarter toutes les cartes sauf les 4 As, qui doivent finir seuls, un par colonne.

• Au départ, une carte sur chacune des 4 colonnes ; le reste forme le talon, à gauche.
• Une carte du dessus d'une colonne peut être écartée si une autre colonne montre une \
carte plus haute de la même couleur. L'As est la carte la plus haute.
• Une colonne vide reçoit la carte du dessus de n'importe quelle autre colonne.
• « Distribuer » (Entrée, ou un clic sur le talon) pose une carte sur chaque colonne.
• Un clic sur une carte l'écarte si elle peut l'être, sinon l'envoie sur une colonne vide.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups possibles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_FORTY_EIGHT: &str = "\
But : monter les 104 cartes (2 jeux) sur les 8 fondations, de l'As au Roi dans la même couleur.

• Au départ, 8 colonnes de 4 cartes visibles ; le reste forme le talon, en bas à droite.
• Sur une colonne, on descend dans la même couleur. Une colonne vide reçoit n'importe quelle \
carte.
• Une suite de même couleur se déplace d'un bloc : 1 carte sans colonne vide, 2 avec une, \
4 avec deux, 8 avec trois… (la colonne d'arrivée, si elle est vide, ne compte pas).
• « Piocher » (Tab, ou un clic sur le talon) retourne une carte sur le rebut ; seule celle \
du dessus se joue. Talon vide, le rebut se retourne en talon, une seule fois.
• Les cartes qui peuvent aller sur une fondation y vont toutes seules (dépôt automatique).
• Double-cliquez sur une carte pour l'envoyer sur une fondation ou une colonne qui la prend.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_BAKERS: &str = "\
But : monter les 52 cartes sur les 4 fondations, de l'As au Roi dans la même couleur.

• 13 colonnes de 4 cartes ; dans chaque colonne, les Rois descendent au fond. Pas de cellule.
• Selon la variante (menu Configuration) : les suites descendent au rang seul, dans la même \
couleur ou en alternant rouge et noir ; une colonne vide reçoit n'importe quelle carte, un \
Roi seulement, ou rien ; dans « Châteaux en Espagne », les cartes sont posées face cachée, \
sauf celle du dessus.
• Une suite se déplace d'un bloc : 1 carte sans colonne vide, 2 avec une, 4 avec deux… \
(seulement si les colonnes vides reçoivent n'importe quelle carte).
• Les cartes qui ne servent plus sur les colonnes vont toutes seules sur les fondations \
(dépôt automatique).
• Double-cliquez sur une carte pour l'envoyer sur une fondation ou une colonne qui la prend.

Variantes : Treize à la douzaine (rang seul, colonnes vides inutilisables), Patience \
espagnole (rang seul, colonnes vides libres), Châteaux en Espagne (couleurs alternées, cartes \
cachées), Solitaire portugais (rang seul, Rois seulement sur les colonnes vides).

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_CASTLE: &str = "\
But : monter les 52 cartes sur les 4 fondations, de l'As au Roi dans la même couleur.

• Les fondations sont au milieu, les colonnes de part et d'autre, deux par rangée.
• Selon la variante (menu Configuration) : les As sont posés d'avance sur les fondations, et \
même (Citadelle, Rois exilés) les cartes qui les suivent pendant la distribution ; une ou \
plusieurs réserves reçoivent chacune une carte.
• Sur les colonnes, on descend au rang seul (ou selon l'option, dans la même couleur ou en \
alternant rouge et noir). Une colonne vide reçoit n'importe quelle carte (Rois exilés : un \
Roi seulement).
• Une suite se déplace d'un bloc si les réserves et les colonnes vides le permettraient \
carte par carte.
• Les cartes qui ne servent plus sur les colonnes vont toutes seules sur les fondations \
(dépôt automatique).
• Double-cliquez sur une carte pour l'envoyer sur une fondation, une réserve ou une colonne.

Variantes : Château assiégé (As d'avance), Citadelle (As et cartes suivantes d'avance), Rois \
exilés (comme Citadelle, colonnes vides aux Rois), Rues et allées (sans As d'avance), Engin de \
siège (une réserve), Forteresse (une réserve, sans As d'avance).

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";

const RULES_NORMANDY: &str = "\
But : monter les 104 cartes (2 jeux) sur les 8 fondations, de l'As au Roi dans la même couleur.

• 8 colonnes d'une carte cachée et de deux visibles ; le reste forme le talon.
• Sur une colonne, on descend en alternant rouge et noir. Toute suite ainsi rangée se \
déplace d'un bloc, quelle que soit sa longueur ; une colonne vide reçoit n'importe quelle \
suite.
• La carte du dessus d'une fondation peut redescendre sur une colonne.
• « Distribuer » (Entrée, ou un clic sur le talon) pose une carte sur chaque colonne ; le \
talon ne se refait pas.
• Les cartes qui ne servent plus sur les colonnes vont toutes seules sur les fondations \
(dépôt automatique).
• Double-cliquez sur une carte pour l'envoyer sur une fondation ou une colonne qui la prend.

Conseil (H) : quand l'outil de résolution sait comment gagner, il montre le prochain coup \
gagnant ; sinon, les coups utiles.
Démonstration (D) : joue les coups gagnants à votre place.";
