//! packarch-spider : le Spider, le FreeCell, le Klondike, l'horloge du
//! grand-père, Simon le simple, le Golf, le Yukon, Grand-père et Mod3 de
//! KPatience, en Rust (egui).
//!
//! Sans argument : la fenêtre du jeu.
//! `--solve COULEURS DÉBUT FIN` et `--solve-freecell VARIANTE DÉBUT FIN` :
//! passent les donnes DÉBUT à FIN dans l'outil de résolution (gagnable /
//! perdue / indéterminée), pour les tests.

mod app;
mod assets;
mod board;
mod cards;
mod clock;
mod clock_solver;
mod freecell;
mod freecell_solver;
mod golf;
mod golf_solver;
mod grandf;
mod grandf_solver;
mod klondike;
mod klondike_solver;
mod mod3;
mod mod3_solver;
mod patsolve;
mod simon;
mod simon_solver;
mod spider;
mod spider_solver;
mod storage;
mod yukon;
mod yukon_solver;

use std::process::ExitCode;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use board::Variant;

const USAGE: &str = "\
Usage : packarch-spider [OPTION]

  (sans option)                       lance le jeu
  --deal N [--suits COULEURS]         lance le Spider sur la donne N (1 à 4 couleurs)
  --deal N --freecell VARIANTE        lance le FreeCell sur la donne N (variante 0 à 4 :
                                      Baker's Game, Eight Off, Forecell, FreeCell,
                                      Seahaven Towers)
  --solve COULEURS DÉBUT FIN          résout les donnes DÉBUT à FIN du Spider (1 à 4 couleurs)
  --deal N --klondike PIOCHE          lance le Klondike sur la donne N (pioche 1 ou 3)
  --deal N --clock                    lance l'horloge du grand-père sur la donne N
  --deal N --simon                    lance Simon le simple sur la donne N
  --deal N --golf                     lance le Golf sur la donne N
  --deal N --yukon                    lance le Yukon sur la donne N
  --deal N --grandfather              lance Grand-père sur la donne N
  --deal N --mod3                     lance Mod3 sur la donne N
  --solve-freecell VARIANTE DÉBUT FIN résout les donnes DÉBUT à FIN du FreeCell
  --solve-klondike PIOCHE DÉBUT FIN   résout les donnes DÉBUT à FIN du Klondike
  --solve-clock DÉBUT FIN             résout les donnes DÉBUT à FIN de l'horloge du grand-père
  --solve-simon DÉBUT FIN             résout les donnes DÉBUT à FIN de Simon le simple
  --solve-golf DÉBUT FIN              résout les donnes DÉBUT à FIN du Golf
  --solve-yukon DÉBUT FIN             résout les donnes DÉBUT à FIN du Yukon
  --solve-grandfather DÉBUT FIN       résout les donnes DÉBUT à FIN de Grand-père
  --solve-mod3 DÉBUT FIN              résout les donnes DÉBUT à FIN de Mod3
  -h, --help                          affiche cette aide
  -V, --version                       affiche la version";

fn solve_range(variant: Variant, start: u32, end: u32) {
    let stop = AtomicBool::new(false);
    let (mut won, mut lost, mut unknown) = (0, 0, 0);
    let total = Instant::now();
    for deal in start..=end {
        let g = match variant {
            Variant::Spider(s) => board::AnyGame::Spider(spider::Game::new(s, deal, false)),
            Variant::FreeCell(v) => board::AnyGame::FreeCell(freecell::Game::new(freecell::PRESETS[v].1, deal)),
            Variant::Klondike(d) => board::AnyGame::Klondike(klondike::Game::new(d, deal)),
            Variant::Clock => board::AnyGame::Clock(clock::Game::new(deal)),
            Variant::Simon => board::AnyGame::Simon(simon::Game::new(deal)),
            Variant::Golf => board::AnyGame::Golf(golf::Game::new(deal)),
            Variant::Yukon => board::AnyGame::Yukon(yukon::Game::new(deal)),
            Variant::Grandfather => board::AnyGame::Grandfather(grandf::Game::new(deal)),
            Variant::Mod3 => board::AnyGame::Mod3(mod3::Game::new(deal)),
        };
        let t = Instant::now();
        let result = match board::solve(&g, &stop) {
            board::Outcome::Winnable(m) => {
                let mut end = g.clone();
                let ok = m.iter().all(|&mv| end.play(mv)) && end.is_won();
                won += 1;
                format!("won{} ({} moves)", if ok { "" } else { " BUT REPLAY FAILED" }, m.len())
            }
            board::Outcome::NotWinnable => {
                lost += 1;
                "lost".into()
            }
            _ => {
                unknown += 1;
                "unknown".into()
            }
        };
        println!("{deal} {result} ({} ms)", t.elapsed().as_millis());
    }
    eprintln!("{won} gagnées, {lost} perdues, {unknown} indéterminées, en {} ms", total.elapsed().as_millis());
}

fn usage_error() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let num = |s: &str| s.parse::<u32>().ok();
    let mut start: Option<(Option<Variant>, u32)> = None;
    match a.as_slice() {
        [] => {}
        ["-h" | "--help"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        ["-V" | "--version"] => {
            println!("packarch-spider {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        ["--solve", suits, first, last] => match (num(suits), num(first), num(last)) {
            (Some(s @ 1..=4), Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Spider(s as u8), x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--solve-freecell", v, first, last] => match (num(v), num(first), num(last)) {
            (Some(v @ 0..=4), Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::FreeCell(v as usize), x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--solve-klondike", d, first, last] => match (num(d), num(first), num(last)) {
            (Some(d @ (1 | 3)), Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Klondike(d as u8), x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--solve-clock", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Clock, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--solve-simon", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Simon, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--solve-mod3", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Mod3, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--deal", deal, "--mod3"] | ["--mod3", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Mod3), d)),
            _ => return usage_error(),
        },
        ["--solve-grandfather", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Grandfather, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--deal", deal, "--grandfather"] | ["--grandfather", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Grandfather), d)),
            _ => return usage_error(),
        },
        ["--solve-yukon", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Yukon, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--deal", deal, "--yukon"] | ["--yukon", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Yukon), d)),
            _ => return usage_error(),
        },
        ["--solve-golf", first, last] => match (num(first), num(last)) {
            (Some(x), Some(y)) if x >= 1 && x <= y => {
                solve_range(Variant::Golf, x, y);
                return ExitCode::SUCCESS;
            }
            _ => return usage_error(),
        },
        ["--deal", deal, "--golf"] | ["--golf", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Golf), d)),
            _ => return usage_error(),
        },
        ["--deal", deal, "--simon"] | ["--simon", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Simon), d)),
            _ => return usage_error(),
        },
        ["--deal", deal, "--clock"] | ["--clock", "--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((Some(Variant::Clock), d)),
            _ => return usage_error(),
        },
        ["--deal", deal] => match num(deal) {
            Some(d) if d >= 1 => start = Some((None, d)),
            _ => return usage_error(),
        },
        ["--deal", deal, opt, value] | [opt, value, "--deal", deal] => match (num(deal), *opt, num(value)) {
            (Some(d), "--suits", Some(s @ 1..=4)) if d >= 1 => start = Some((Some(Variant::Spider(s as u8)), d)),
            (Some(d), "--freecell", Some(v @ 0..=4)) if d >= 1 => start = Some((Some(Variant::FreeCell(v as usize)), d)),
            (Some(d), "--klondike", Some(n @ (1 | 3))) if d >= 1 => start = Some((Some(Variant::Klondike(n as u8)), d)),
            _ => return usage_error(),
        },
        _ => return usage_error(),
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Packarch Spider")
            .with_app_id("org.packarch.Spider")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };
    match eframe::run_native("Packarch Spider", options, Box::new(move |cc| Ok(Box::new(app::SpiderApp::new(cc, start))))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("packarch-spider : {e}");
            ExitCode::FAILURE
        }
    }
}
