//! Réglages, statistiques et partie en cours, en JSON :
//! - `~/.config/packarch-spider/settings.json` et `stats.json` ;
//! - `~/.local/share/packarch-spider/state.json` (partie en cours, si
//!   « Se souvenir de l'état de la partie à sa fermeture » est coché).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::board::{AnyGame, GameKind, Variant};
use crate::freecell;
use crate::spider;

const APP: &str = "packarch-spider";

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
            home.join(fallback)
        }
    }
}

pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join(APP)
}

pub fn data_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share").join(APP)
}

fn load<T: DeserializeOwned + Default>(path: PathBuf) -> T {
    fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save<T: Serialize>(path: PathBuf, value: &T) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            // Écriture atomique : fichier temporaire puis renommage.
            let tmp = path.with_extension("json.tmp");
            if fs::write(&tmp, s).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
        Err(e) => eprintln!("Impossible d'enregistrer {} : {e}", path.display()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Le jeu choisi.
    pub game: GameKind,
    /// Spider : 1 à 4 couleurs.
    pub suit_count: u8,
    pub stacks_face_up: bool,
    pub solver_enabled: bool,
    pub remember_state: bool,
    pub confirm_abandon: bool,
    pub card_deck: String,
    pub theme: String,
    pub show_toolbar: bool,
    /// Texte à côté des icônes de la barre d'outils.
    pub toolbar_text: bool,
    /// FreeCell : variante et options.
    pub freecell: freecell::Options,
    /// FreeCell et Klondike : dépôt automatique sur les fondations.
    pub autodrop: bool,
    /// Klondike : cartes retournées à chaque pioche (1 ou 3).
    pub klondike_draw: u8,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            suit_count: 2,
            stacks_face_up: false,
            solver_enabled: true,
            remember_state: false,
            confirm_abandon: true,
            card_deck: "svg-oxygen-air".into(),
            theme: "greenblaze".into(),
            show_toolbar: true,
            toolbar_text: false,
            game: GameKind::Spider,
            freecell: freecell::Options::default(),
            autodrop: true,
            klondike_draw: 1,
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = load(config_dir().join("settings.json"));
        s.suit_count = s.suit_count.clamp(1, 4);
        s.freecell = s.freecell.sanitized();
        if s.klondike_draw != 3 {
            s.klondike_draw = 1;
        }
        s
    }

    pub fn save(&self) {
        save(config_dir().join("settings.json"), self);
    }
}

/// Statistiques d'une variante.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VariantStats {
    pub played: u32,
    pub won: u32,
    pub win_streak: u32,
    pub max_win_streak: u32,
    pub lose_streak: u32,
    pub max_lose_streak: u32,
    pub min_moves: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Stats {
    /// Spider. Indice 0 : 1 couleur, …, indice 3 : 4 couleurs.
    pub variants: [VariantStats; 4],
    /// FreeCell, par variante (`freecell::PRESETS`, puis personnalisée).
    pub freecell: [VariantStats; freecell::VARIANTS],
    /// Klondike, pioche 1 puis pioche 3.
    pub klondike: [VariantStats; 2],
    /// L'horloge du grand-père.
    pub clock: VariantStats,
    /// Simon le simple.
    pub simon: VariantStats,
    /// Golf.
    pub golf: VariantStats,
    /// Yukon.
    pub yukon: VariantStats,
    /// Grand-père.
    pub grandfather: VariantStats,
    /// Mod3.
    pub mod3: VariantStats,
    /// Les as.
    pub aces_up: VariantStats,
    /// Quarante & Huit.
    pub forty_eight: VariantStats,
}

impl Stats {
    pub fn load() -> Stats {
        load(config_dir().join("stats.json"))
    }

    pub fn save(&self) {
        save(config_dir().join("stats.json"), self);
    }

    pub fn get(&self, variant: Variant) -> &VariantStats {
        match variant {
            Variant::Spider(n) => &self.variants[(n.clamp(1, 4) - 1) as usize],
            Variant::FreeCell(v) => &self.freecell[v.min(freecell::CUSTOM)],
            Variant::Klondike(d) => &self.klondike[usize::from(d == 3)],
            Variant::Clock => &self.clock,
            Variant::Simon => &self.simon,
            Variant::Golf => &self.golf,
            Variant::Yukon => &self.yukon,
            Variant::Grandfather => &self.grandfather,
            Variant::Mod3 => &self.mod3,
            Variant::AcesUp => &self.aces_up,
            Variant::FortyEight => &self.forty_eight,
        }
    }

    fn get_mut(&mut self, variant: Variant) -> &mut VariantStats {
        match variant {
            Variant::Spider(n) => &mut self.variants[(n.clamp(1, 4) - 1) as usize],
            Variant::FreeCell(v) => &mut self.freecell[v.min(freecell::CUSTOM)],
            Variant::Klondike(d) => &mut self.klondike[usize::from(d == 3)],
            Variant::Clock => &mut self.clock,
            Variant::Simon => &mut self.simon,
            Variant::Golf => &mut self.golf,
            Variant::Yukon => &mut self.yukon,
            Variant::Grandfather => &mut self.grandfather,
            Variant::Mod3 => &mut self.mod3,
            Variant::AcesUp => &mut self.aces_up,
            Variant::FortyEight => &mut self.forty_eight,
        }
    }

    pub fn record(&mut self, variant: Variant, won: bool, moves: u32) {
        let v = self.get_mut(variant);
        v.played += 1;
        if won {
            v.won += 1;
            v.win_streak += 1;
            v.lose_streak = 0;
            v.max_win_streak = v.max_win_streak.max(v.win_streak);
            v.min_moves = Some(v.min_moves.map_or(moves, |m| m.min(moves)));
        } else {
            v.lose_streak += 1;
            v.win_streak = 0;
            v.max_lose_streak = v.max_lose_streak.max(v.lose_streak);
        }
        self.save();
    }

    pub fn reset(&mut self, variant: Variant) {
        *self.get_mut(variant) = VariantStats::default();
        self.save();
    }
}

/// La partie en cours, pour la reprendre au prochain lancement.
#[derive(Serialize, Deserialize)]
pub struct SavedState {
    pub game: AnyGame,
    pub help_used: bool,
}

impl Default for SavedState {
    fn default() -> SavedState {
        SavedState { game: AnyGame::Spider(spider::Game::new(2, 1, false)), help_used: false }
    }
}

pub fn load_state() -> Option<SavedState> {
    let path = data_dir().join("state.json");
    let s = fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn save_state(state: &SavedState) {
    save(data_dir().join("state.json"), state);
}

pub fn clear_state() {
    let _ = fs::remove_file(data_dir().join("state.json"));
}
