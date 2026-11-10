//! Outil de résolution de Normandie : portage de `NormandySolver` de
//! KPatience (© 2006-2009 Stephan Kulow, GPL-2.0-or-later) sur le moteur
//! `patsolve`, avec ses priorités (`params`) et ses élagages.
//!
//! Comme l'original, il connaît les cartes face cachée. Piles de travail :
//! les 8 colonnes et le talon (8) ; les fondations du jeu sont suivies une à
//! une (leur carte du dessus), comparées aux symétries près (hauteurs triées
//! par couleur), comme pour Quarante & Huit.

use std::sync::atomic::AtomicBool;

use crate::board::{Move, Outcome};
use crate::cards::{Card, Suit};
use crate::normandy::{FOUNDATION, Game, STORES, TALON};
use crate::patsolve::{self, DOWN, Piles};

/// Au-delà, la recherche abandonne (comme pour le Klondike).
const MAX_POSITIONS: usize = 1_000_000;

const DECK: usize = STORES;
const ACE: u8 = 1;
const KING: u8 = 13;
/// `NormandySolver::params` : distribuer, suite déjà bien posée qui libère
/// une sortie (+ 1 par carte), coup qui retourne une carte, autre coup,
/// sortie (127 : toujours automatique), facteur de la longueur.
const PARAMS: [i32; 6] = [3, 3, 44, 19, 5, 10];

// Codage de patsolve : (face cachée << 7) + (couleur << 4) + rang, couleurs
// dans l'ordre carreau, trèfle, cœur, pique (le bit 4 vaut « noir »).
fn rank(c: u8) -> u8 {
    c & 0xF
}
fn suit(c: u8) -> u8 {
    (c >> 4) & 3
}
fn black(c: u8) -> bool {
    c & 0x10 != 0
}
fn down(c: u8) -> bool {
    c & DOWN != 0
}

fn code(c: &Card) -> u8 {
    let s = match c.suit {
        Suit::Diamonds => 0,
        Suit::Clubs => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    };
    s << 4 | c.rank | if c.face_up { 0 } else { DOWN }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// La carte du dessus de `from` vers la fondation `f` du jeu (0 à 7).
    Out {
        from: usize,
        f: usize,
    },
    /// `card_index` + 1 cartes de `from` vers la colonne `to`.
    Store {
        from: usize,
        to: usize,
        card_index: usize,
    },
    Deal,
}

#[derive(Clone, Copy, Debug)]
struct Mv {
    kind: Kind,
    pri: i32,
}

/// Une position, sans allocation.
#[derive(Clone, Copy)]
struct State {
    w: Piles<{ STORES + 1 }, 104>,
    /// Le dessus de chaque fondation du jeu (0 : vide).
    f: [u8; 8],
}

impl State {
    fn from_game(g: &Game) -> State {
        let piles = (0..STORES).chain([TALON]).map(|p| g.piles[p].iter().map(code));
        let f = std::array::from_fn(|i| g.piles[FOUNDATION + i].last().map_or(0, |c| code(c) & !DOWN));
        State { w: Piles::new(piles), f }
    }

    fn pile(&self, i: usize) -> &[u8] {
        self.w.pile(i)
    }

    /// Hauteurs des fondations de la couleur `s`, triées (2 valeurs).
    fn suit_heights(&self, s: u8) -> [u8; 2] {
        let mut h = [0u8; 2];
        for &t in self.f.iter().filter(|&&t| t != 0 && suit(t) == s) {
            if rank(t) > h[0] {
                h[1] = h[0];
                h[0] = rank(t);
            } else if rank(t) > h[1] {
                h[1] = rank(t);
            }
        }
        h
    }

    fn heights(&self) -> [u8; 8] {
        let mut h = [0u8; 8];
        for s in 0..4u8 {
            let [a, b] = self.suit_heights(s);
            h[s as usize * 2] = a;
            h[s as usize * 2 + 1] = b;
        }
        h
    }

    fn outs(&self) -> i32 {
        self.f.iter().map(|&c| rank(c) as i32).sum()
    }

    fn is_won(&self) -> bool {
        self.f.iter().all(|&c| rank(c) == KING)
    }

    fn key(&self) -> u64 {
        let mut h = patsolve::FNV_START;
        self.w.hash(&mut h);
        patsolve::fnv(&mut h, &self.heights());
        h
    }

    /// La première fondation du jeu qui prend `card`.
    fn foundation_for(&self, card: u8) -> Option<usize> {
        let card = card & !DOWN;
        self.f.iter().position(|&t| if rank(card) == ACE { t == 0 } else { t != 0 && t + 1 == card })
    }

    /// `NormandySolver::good_automove` : règle de Raymond, 4 fondations par
    /// teinte (carreau et cœur rouges, trèfle et pique noirs).
    fn good_automove(&self, card: u8) -> bool {
        let r = rank(card);
        if r <= 2 {
            return true;
        }
        let tint =
            |is_black: bool| -> Vec<u8> { (0..4u8).filter(|&s| (s & 1 == 1) == is_black).flat_map(|s| self.suit_heights(s)).collect() };
        let opposite = tint(!black(card));
        if opposite.iter().all(|&h| h + 1 >= r) {
            return true;
        }
        opposite.iter().all(|&h| h + 2 >= r) && tint(black(card)).iter().all(|&h| h + 3 >= r)
    }

    /// `NormandySolver::get_possible_moves` : (coups, nombre de sorties
    /// non automatiques en tête de liste) ; une sortie automatique est seule.
    fn possible_moves(&self) -> (Vec<Mv>, usize) {
        let mut out = Vec::new();
        for w in 0..STORES {
            let Some(&card) = self.pile(w).last() else { continue };
            if let Some(f) = self.foundation_for(card) {
                let m = Mv { kind: Kind::Out { from: w, f }, pri: PARAMS[4] };
                if PARAMS[4] == 127 || self.good_automove(card) {
                    return (vec![Mv { pri: 127, ..m }], 0);
                }
                out.push(m);
            }
        }
        let numout = out.len();

        for i in 0..STORES {
            let p = self.pile(i);
            let len = p.len();
            for l in 0..len {
                let card = p[len - 1 - l];
                if down(card) {
                    break;
                }
                if l > 0 {
                    let on_top = p[len - l];
                    if rank(card) != rank(on_top) + 1 || black(card) == black(on_top) {
                        break;
                    }
                }
                let mut was_empty = false;
                for j in 0..STORES {
                    if i == j {
                        continue;
                    }
                    let pj = self.pile(j);
                    let allowed = match pj.last() {
                        Some(&t) => black(card) != black(t) && rank(card) + 1 == rank(t),
                        None if !was_empty && l != len - 1 => {
                            was_empty = true;
                            true
                        }
                        None => false,
                    };
                    if !allowed {
                        continue;
                    }
                    let mut pri = PARAMS[3];
                    if len >= 2 + l {
                        let card2 = p[len - 2 - l];
                        if down(card2) {
                            pri = PARAMS[2];
                        } else if rank(card) + 1 == rank(card2) && black(card) != black(card2) {
                            // Suite déjà bien posée : la déplacer ne sert qu'à
                            // libérer une sortie.
                            pri =
                                if self.foundation_for(card2).is_some() { (PARAMS[1] + l as i32 * PARAMS[5] / 10).min(127) } else { -117 };
                        }
                    }
                    out.push(Mv { kind: Kind::Store { from: i, to: j, card_index: l }, pri });
                }
            }
        }

        if !self.pile(DECK).is_empty() {
            out.push(Mv { kind: Kind::Deal, pri: PARAMS[0] });
        }
        (out, numout)
    }

    fn apply(&self, m: &Mv) -> State {
        let mut s = *self;
        match m.kind {
            Kind::Out { from, f } => {
                s.f[f] = self.pile(from).last().expect("carte") & !DOWN;
                s.w.remove(from, 1);
                s.w.turn_top(from);
            }
            Kind::Store { from, to, card_index } => {
                s.w.transfer(from, to, card_index + 1);
                s.w.turn_top(from);
            }
            Kind::Deal => {
                for i in 0..STORES {
                    if s.w.pile(DECK).is_empty() {
                        break;
                    }
                    s.w.transfer(DECK, i, 1);
                    s.w.turn_top(i);
                }
            }
        }
        s
    }

    fn game_move(&self, m: &Mv) -> Move {
        match m.kind {
            Kind::Out { from, f } => Move::Cards { from, index: self.pile(from).len() - 1, to: FOUNDATION + f },
            Kind::Store { from, to, card_index } => Move::Cards { from, index: self.pile(from).len() - card_index - 1, to },
            Kind::Deal => Move::Deal,
        }
    }
}

impl patsolve::Position for State {
    type Mv = Mv;

    /// `Solver::get_moves` : les sorties qui ne sont pas automatiques passent
    /// après les autres coups.
    fn moves(&self) -> Vec<(Mv, i32)> {
        let (all, numout) = self.possible_moves();
        all[numout..].iter().chain(&all[..numout]).map(|&m| (m, m.pri)).collect()
    }
    fn apply(&self, m: &Mv) -> State {
        State::apply(self, m)
    }
    fn game_move(&self, m: &Mv) -> Move {
        State::game_move(self, m)
    }
    fn outs(&self) -> i32 {
        State::outs(self)
    }
    /// KPatience ne redéfinit pas `getClusterNumber` pour Normandie (0) ;
    /// ici les fondations, pour explorer tout de suite une position où une
    /// carte vient de sortir : 81 donnes gagnées sur 100 au lieu de 73.
    fn cluster(&self) -> u32 {
        self.heights().iter().fold(0, |k, &x| k.wrapping_mul(14).wrapping_add(x as u32))
    }
    fn key(&self) -> u64 {
        State::key(self)
    }
    fn is_won(&self) -> bool {
        State::is_won(self)
    }
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    patsolve::solve(State::from_game(game), stop, MAX_POSITIONS)
}

/// Les coups utiles de la position (ceux que l'outil de résolution
/// essaierait, sans les suites déplacées pour rien), pour les conseils quand
/// il ne connaît pas de chemin gagnant.
pub fn useful_moves(game: &Game) -> Vec<Move> {
    let s = State::from_game(game);
    let (moves, _) = s.possible_moves();
    moves.iter().filter(|m| m.pri > 0).map(|m| s.game_move(m)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in 1..=20 {
            let g = Game::new(deal);
            if let Outcome::Winnable(moves) = solve(&g, &stop) {
                let mut end = g.clone();
                for (k, &m) in moves.iter().enumerate() {
                    assert!(end.play(m), "donne {deal}, coup {k} illégal : {m:?}");
                }
                assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                solved += 1;
            }
        }
        assert!(solved >= 5, "{solved} donnes résolues sur 20");
    }

    #[test]
    fn useful_moves_are_legal() {
        for deal in 1..=20 {
            let mut g = Game::new(deal);
            for _ in 0..30 {
                let moves = useful_moves(&g);
                for &m in &moves {
                    assert!(g.is_legal(m), "donne {deal} : {m:?}");
                }
                let Some(&m) = moves.first() else { break };
                g.play(m);
            }
        }
    }

    /// Les règles de Raymond du jeu et du solveur sont d'accord.
    #[test]
    fn automove_rules_agree() {
        for deal in 1..=30 {
            let mut g = Game::new(deal);
            for _ in 0..60 {
                let s = State::from_game(&g);
                for col in &g.piles[..STORES] {
                    if let Some(c) = col.last() {
                        assert_eq!(s.good_automove(code(c)), crate::normandy::good_automove(&g.piles, c.suit, c.rank));
                    }
                }
                let Some(&m) = useful_moves(&g).first() else { break };
                g.play(m);
            }
        }
    }
}
