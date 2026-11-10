//! Outil de résolution du Spider : portage de `spidersolver2` de KPatience
//! (© 2023 Stephan Kulow, GPL-2.0-or-later).
//!
//! Recherche en largeur, niveau par niveau : à chaque profondeur, les
//! positions nouvelles sont triées par « désordre » (chaos) croissant et
//! seules les `LEVEL_CAP` meilleures par nombre de rangées restantes sont
//! gardées. Comme l'original, il connaît les cartes face cachée et l'ordre
//! de la réserve : s'il trouve une solution, la partie peut vraiment être
//! gagnée.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;

use crate::board;
use crate::spider::{self as game, Game};

const REDEALS_NR: usize = 6;
const MAX_MOVES: usize = 230;
const AVG_OPTIONS: usize = 50;
const LEVEL_CAP: usize = 150;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Gagnable : la suite des coups, depuis la position donnée.
    Winnable(Vec<SolverMove>),
    NotWinnable,
    /// Recherche trop large : impossible de conclure.
    Unknown,
    /// Arrêtée avant la fin (la position a changé).
    Aborted,
}

/// Un coup du solveur. Les sorties de suites complètes (`Off`) sont faites
/// automatiquement par le jeu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverMove {
    Cards { from: usize, index: usize, to: usize },
    Deal,
    Off { from: usize },
}

impl SolverMove {
    pub fn to_game_move(self) -> Option<board::Move> {
        match self {
            SolverMove::Cards { from, index, to } => Some(board::Move::Cards { from, index, to }),
            SolverMove::Deal => Some(board::Move::Deal),
            SolverMove::Off { .. } => None,
        }
    }
}

// ---------------------------------------------------------------- cartes

/// Même codage que l'original : 4 bits de rang, 2 de couleur, 1 bit « face
/// visible ».
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
struct Card(u8);

impl Card {
    fn new(rank: u8, suit: u8, face_up: bool) -> Card {
        Card((rank & 15) | ((suit & 3) << 4) | if face_up { 1 << 6 } else { 0 })
    }
    fn rank(self) -> u8 {
        self.0 & 15
    }
    fn suit(self) -> u8 {
        (self.0 >> 4) & 3
    }
    fn face_up(self) -> bool {
        self.0 & (1 << 6) != 0
    }
    fn with_face_up(self) -> Card {
        Card(self.0 | (1 << 6))
    }
    /// `self` peut-il prolonger une suite dont `other` est la carte précédente ?
    fn in_sequence_to(self, other: Card) -> bool {
        other.face_up() && other.suit() == self.suit() && other.rank() == self.rank() + 1
    }
}

// ---------------------------------------------------------------- piles

struct Pile {
    cards: Vec<Card>,
    chaos: i32,
    seqs: [i32; 4],
    hash: u64,
}

impl Pile {
    fn build(cards: Vec<Card>) -> Pile {
        let mut seed: u64 = 0;
        for c in &cards {
            seed ^= (c.0 as u64).wrapping_add(0x9e3779b9).wrapping_add(seed << 6).wrapping_add(seed >> 2);
        }
        let mut p = Pile { cards, chaos: 0, seqs: [0; 4], hash: seed };
        p.chaos = p.compute_chaos();
        for s in 0..4u8 {
            p.seqs[s as usize] = p.compute_sequence_of(s);
        }
        p
    }

    fn len(&self) -> usize {
        self.cards.len()
    }

    fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    fn at(&self, i: usize) -> Card {
        self.cards[i]
    }

    fn compute_chaos(&self) -> i32 {
        let mut chaos = 0;
        let mut last: Option<Card> = None;
        for &current in &self.cards {
            match last {
                None => chaos += 1,
                Some(l) if !current.in_sequence_to(l) => chaos += 1,
                _ => {}
            }
            last = Some(current);
        }
        chaos
    }

    fn compute_sequence_of(&self, suit: u8) -> i32 {
        let count = self.len();
        if count == 0 {
            return 0;
        }
        let mut index = count - 1;
        let mut top = self.at(index);
        if top.suit() != suit {
            return 0;
        }
        while index > 0 && top.in_sequence_to(self.at(index - 1)) {
            index -= 1;
            top = self.at(index);
        }
        (count - index) as i32
    }

    fn sequence_of(&self, suit: u8) -> i32 {
        self.seqs[suit as usize]
    }

    fn playable_cards(&self) -> i32 {
        if self.len() < 2 {
            return self.len() as i32;
        }
        self.sequence_of(self.at(self.len() - 1).suit())
    }
}

/// Ce que la recherche doit savoir d'une pile pour classer une position :
/// calculable directement sur les cartes, sans construire la pile.
#[derive(Clone, Copy)]
struct View {
    hash: u64,
    chaos: i32,
    playable: i32,
    len: usize,
    first: Option<Card>,
    top: Option<Card>,
}

impl View {
    /// Les mêmes calculs que `Pile::build`, `Pile::playable_cards`.
    fn of(cards: &[Card]) -> View {
        let mut hash: u64 = 0;
        let mut chaos = 0;
        let mut last: Option<Card> = None;
        for &c in cards {
            hash ^= (c.0 as u64).wrapping_add(0x9e3779b9).wrapping_add(hash << 6).wrapping_add(hash >> 2);
            match last {
                Some(l) if c.in_sequence_to(l) => {}
                _ => chaos += 1,
            }
            last = Some(c);
        }
        let playable = if cards.len() < 2 {
            cards.len() as i32
        } else {
            let mut i = cards.len() - 1;
            while i > 0 && cards[i].in_sequence_to(cards[i - 1]) {
                i -= 1;
            }
            (cards.len() - i) as i32
        };
        View { hash, chaos, playable, len: cards.len(), first: cards.first().copied(), top: cards.last().copied() }
    }
}

impl Pile {
    fn view(&self) -> View {
        View {
            hash: self.hash,
            chaos: self.chaos,
            playable: self.playable_cards(),
            len: self.len(),
            first: self.cards.first().copied(),
            top: self.cards.last().copied(),
        }
    }
}

/// Construction des piles. Une position nouvelle ne recrée que les piles
/// qui changent ; les autres restent partagées (`Arc`) avec la position
/// d'origine. (L'original met aussi en commun les piles identiques venues de
/// chemins différents ; ce n'est qu'une économie de mémoire, et sans elle les
/// fils de calcul n'ont rien à partager d'autre que des piles immuables.)
struct PileStore;

impl PileStore {
    fn get(&self, cards: Vec<Card>) -> Arc<Pile> {
        Arc::new(Pile::build(cards))
    }

    fn empty(&self) -> Arc<Pile> {
        self.get(Vec::new())
    }

    /// Retire les cartes à partir de `index` et retourne la nouvelle carte du dessus.
    fn remove(&self, pile: &Pile, index: usize) -> Arc<Pile> {
        let mut cards = pile.cards[..index].to_vec();
        if let Some(last) = cards.last_mut() {
            *last = last.with_face_up();
        }
        self.get(cards)
    }

    fn copy_from(&self, pile: &Pile, from: &Pile, index: usize) -> Arc<Pile> {
        let mut cards = Vec::with_capacity(pile.len() + from.len() - index);
        cards.extend_from_slice(&pile.cards);
        cards.extend_from_slice(&from.cards[index..]);
        self.get(cards)
    }

    fn add_card(&self, pile: &Pile, c: Card) -> Arc<Pile> {
        let mut cards = Vec::with_capacity(pile.len() + 1);
        cards.extend_from_slice(&pile.cards);
        cards.push(c);
        self.get(cards)
    }
}

// ---------------------------------------------------------------- positions

/// Liste chaînée persistante des coups joués depuis la position de départ.
struct MoveNode {
    mv: SolverMove,
    parent: Option<Arc<MoveNode>>,
    len: usize,
}

#[derive(Clone)]
struct Deck {
    play: [Arc<Pile>; 10],
    talon: [Arc<Pile>; 5],
    off: Arc<Pile>,
    moves: Option<Arc<MoveNode>>,
}

impl Deck {
    fn moves_len(&self) -> usize {
        self.moves.as_ref().map_or(0, |m| m.len)
    }

    fn move_list(&self) -> Vec<SolverMove> {
        let mut out = Vec::with_capacity(self.moves_len());
        let mut node = self.moves.clone();
        while let Some(n) = node {
            out.push(n.mv);
            node = n.parent.clone();
        }
        out.reverse();
        out
    }

    fn get_moves(&self, out: &mut Vec<SolverMove>) {
        out.clear();
        if self.moves_len() >= MAX_MOVES - 1 {
            return;
        }

        let next_talon = self.talon.iter().position(|t| !t.is_empty());

        let mut one_is_empty = false;
        for from in 0..10 {
            let pile = &self.play[from];
            if pile.is_empty() {
                one_is_empty = true;
                continue;
            }

            let mut index = pile.len() as isize - 1;
            let top_suit = pile.at(index as usize).suit();
            let mut top_rank = pile.at(index as usize).rank() as i32 - 1;

            while index >= 0 {
                let i = index as usize;
                let current = pile.at(i);
                if !current.face_up() || current.suit() != top_suit || top_rank + 1 != current.rank() as i32 {
                    break;
                }
                top_rank = current.rank() as i32;

                if pile.len() - i == 13 {
                    out.clear();
                    out.push(SolverMove::Off { from });
                    return;
                }

                let mut broken_sequence = 0;
                if i > 0 && current.in_sequence_to(pile.at(i - 1)) {
                    broken_sequence = (pile.len() - i) as i32;
                }

                let mut moved_to_empty = false;
                for to in 0..10 {
                    if to == from {
                        continue;
                    }
                    let dest = &self.play[to];
                    if !dest.is_empty() {
                        let top = dest.at(dest.len() - 1);
                        if top.rank() as i32 != top_rank + 1 {
                            continue;
                        }
                        // Ne faire que des coups qui allongent les suites.
                        if broken_sequence > 0 && dest.sequence_of(top_suit) + broken_sequence <= pile.sequence_of(top_suit) {
                            continue;
                        }
                    } else if moved_to_empty {
                        if next_talon.is_none() {
                            continue;
                        }
                    } else {
                        // Tant qu'il reste des rangées, pas d'optimisation ;
                        // en fin de partie, on élague davantage.
                        if next_talon.is_none() {
                            if i == 0 {
                                // Pas de déplacement d'une colonne vers une colonne vide.
                                continue;
                            }
                            // Aucune raison de couper une suite en fin de partie.
                            if broken_sequence > 0 {
                                continue;
                            }
                        }
                        moved_to_empty = true;
                    }
                    out.push(SolverMove::Cards { from, index: i, to });
                }
                index -= 1;
            }
        }

        if !one_is_empty && next_talon.is_some() {
            out.push(SolverMove::Deal);
        }
    }

    fn apply(&self, m: SolverMove, store: &PileStore) -> Deck {
        let mut d = self.clone();
        d.moves = Some(Arc::new(MoveNode { mv: m, parent: self.moves.clone(), len: self.moves_len() + 1 }));
        match m {
            SolverMove::Deal => {
                let t = d.talon.iter().position(|t| !t.is_empty()).expect("rangée disponible");
                let row = d.talon[t].clone();
                for to in 0..10 {
                    let c = row.at(to).with_face_up();
                    d.play[to] = store.add_card(&d.play[to], c);
                }
                d.talon[t] = store.empty();
            }
            SolverMove::Off { from } => {
                let pile = d.play[from].clone();
                let start = pile.len() - 13;
                d.off = store.add_card(&d.off, pile.at(start));
                d.play[from] = store.remove(&pile, start);
            }
            SolverMove::Cards { from, index, to } => {
                let src = d.play[from].clone();
                d.play[to] = store.copy_from(&d.play[to], &src, index);
                d.play[from] = store.remove(&src, index);
            }
        }
        d
    }

    /// Le classement de la position obtenue en jouant `m`, calculé sans la
    /// construire (seules les positions retenues le seront). `buf` sert de
    /// tampon pour les cartes des piles qui changent.
    fn summary(&self, m: SolverMove, buf: &mut Vec<Card>) -> Weighted {
        let mut play: [View; 10] = std::array::from_fn(|i| self.play[i].view());
        let mut talon: [(u64, bool); 5] = std::array::from_fn(|i| (self.talon[i].hash, !self.talon[i].is_empty()));
        let mut off = self.off.len();
        let mut view_of = |parts: &[&[Card]], turn_top: bool| {
            buf.clear();
            for p in parts {
                buf.extend_from_slice(p);
            }
            if turn_top && let Some(last) = buf.last_mut() {
                *last = last.with_face_up();
            }
            View::of(buf)
        };
        match m {
            SolverMove::Deal => {
                let t = self.talon.iter().position(|t| !t.is_empty()).expect("rangée disponible");
                for (to, v) in play.iter_mut().enumerate() {
                    let c = self.talon[t].at(to).with_face_up();
                    *v = view_of(&[&self.play[to].cards, &[c]], false);
                }
                talon[t] = (View::of(&[]).hash, false);
            }
            SolverMove::Off { from } => {
                let pile = &self.play[from];
                off += 1;
                play[from] = view_of(&[&pile.cards[..pile.len() - 13]], true);
            }
            SolverMove::Cards { from, index, to } => {
                let (src, dst) = (&self.play[from], &self.play[to]);
                play[to] = view_of(&[&dst.cards, &src.cards[index..]], false);
                play[from] = view_of(&[&src.cards[..index]], true);
            }
        }
        summarize(&play, &talon, off, m)
    }

    /// Le classement de cette position (pour les tests).
    #[cfg(test)]
    fn summarize_self(&self) -> Weighted {
        let play: [View; 10] = std::array::from_fn(|i| self.play[i].view());
        let talon: [(u64, bool); 5] = std::array::from_fn(|i| (self.talon[i].hash, !self.talon[i].is_empty()));
        summarize(&play, &talon, self.off.len(), SolverMove::Deal)
    }
}

/// Identifiant et critères de tri d'une position (`Deck::id`, `chaos`… de
/// l'original), à partir de ses piles.
fn summarize(play: &[View; 10], talon: &[(u64, bool); 5], off: usize, mv: SolverMove) -> Weighted {
    let mut id: u64 = 0;
    for h in play.iter().map(|v| v.hash).chain(talon.iter().map(|t| t.0)) {
        id ^= h.wrapping_add(0x9e3779b9).wrapping_add(id << 6).wrapping_add(id >> 2);
    }
    let free_plays = play.iter().filter(|v| v.len == 0).count() as i32;
    // Au moins 1 par colonne non vide, moins 1 si elle peut se raccrocher
    // à une autre colonne ; les victoires évidentes ont un chaos de 0.
    let mut chaos: i32 = play.iter().map(|v| v.chaos).sum();
    for (i, v) in play.iter().enumerate() {
        let Some(c1) = v.first else { continue };
        if c1.rank() == 13 {
            chaos -= 1;
            continue;
        }
        if play.iter().enumerate().any(|(j, w)| j != i && w.top.is_some_and(|top| c1.rank() as i32 == top.rank() as i32 - 1)) {
            chaos -= 1;
        }
    }
    let mut fp = free_plays;
    while fp > 0 && chaos > 0 {
        fp -= 1;
        chaos -= 1;
    }
    Weighted {
        left_talons: talon.iter().filter(|t| t.1).count() as u8,
        in_off: (off * 13) as u8,
        free_plays,
        playable: play.iter().map(|v| v.playable).sum(),
        chaos,
        id,
        parent: 0,
        mv,
    }
}

/// Une position candidate : son classement, et comment l'obtenir (la
/// position `parent` du niveau précédent, plus le coup `mv`).
struct Weighted {
    left_talons: u8,
    in_off: u8,
    free_plays: i32,
    playable: i32,
    chaos: i32,
    id: u64,
    parent: u32,
    mv: SolverMove,
}

/// Les identifiants de positions sont déjà des hachages : un simple
/// brassage suffit (bien plus rapide que le hachage par défaut).
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(8) ^ b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = n.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

type Seen = HashSet<u64, BuildHasherDefault<IdHasher>>;

/// La clé de tri équivalente à `compare` : trier selon elle donne le même
/// ordre, plus vite (les identifiants sont uniques après élimination des
/// doublons, l'ordre est donc total).
fn sort_key(w: &Weighted) -> (i32, i32, i32, u8, i64) {
    let ready = w.playable + w.in_off as i32 + w.free_plays;
    let (free, off) = if w.chaos == 0 { (-w.free_plays, w.in_off) } else { (0, 0) };
    (w.chaos, -ready, free, off, w.id as i64)
}

/// Ordre de l'original : plus petit est meilleur.
#[cfg(test)]
fn compare(a: &Weighted, b: &Weighted) -> std::cmp::Ordering {
    if a.chaos != b.chaos {
        return a.chaos.cmp(&b.chaos);
    }
    let ready1 = a.playable + a.in_off as i32 + a.free_plays;
    let ready2 = b.playable + b.in_off as i32 + b.free_plays;
    if ready1 != ready2 {
        return ready2.cmp(&ready1);
    }
    if a.chaos == 0 {
        if a.free_plays != b.free_plays {
            return b.free_plays.cmp(&a.free_plays);
        }
        if a.in_off != b.in_off {
            return a.in_off.cmp(&b.in_off);
        }
    }
    // Ordre reproductible pour les positions de même valeur ; l'original
    // compare l'identifiant comme un entier signé.
    (a.id as i64).cmp(&(b.id as i64))
}

// ---------------------------------------------------------------- recherche

fn translate(game: &Game, store: &PileStore) -> Deck {
    let to_card = |c: &crate::cards::Card| Card::new(c.rank, c.suit as u8, c.face_up);

    let play: [Arc<Pile>; 10] = std::array::from_fn(|i| store.get(game.columns[i].iter().map(to_card).collect()));
    // Les rangées déjà distribuées sont vides, au début.
    let dealt = game::ROWS - game.stock.len();
    let talon: [Arc<Pile>; 5] =
        std::array::from_fn(|i| if i < dealt { store.empty() } else { store.get(game.stock[i - dealt].iter().map(to_card).collect()) });
    let mut off = store.empty();
    for run in &game.runs {
        off = store.add_card(&off, to_card(&run[0]));
    }
    Deck { play, talon, off, moves: None }
}

/// Au-delà, les fils supplémentaires n'accélèrent plus la recherche
/// (mesuré sur un Ryzen 9 7950X : 8 fils font aussi bien que 16).
const MAX_THREADS: usize = 8;

/// Le pool de fils de calcul, créé une fois : la moitié des cœurs (pour
/// laisser de la place au reste du bureau), 8 au plus, ou
/// `PACKARCH_SOLVER_THREADS`.
fn pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let n = std::env::var("PACKARCH_SOLVER_THREADS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).min(MAX_THREADS)))
            .max(1);
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("solveur-spider-{i}"))
            .build()
            .expect("pool de fils du solveur")
    })
}

/// Positions développées par tâche : assez pour amortir la répartition.
const CHUNK: usize = 8;

/// Classe toutes les positions issues d'un niveau, sans les construire ;
/// le résultat garde l'ordre de l'original (position par position, coup
/// par coup).
/// Les positions déjà vues aux niveaux précédents (`seen`, en lecture seule
/// ici) sont écartées tout de suite.
fn expand_level(unvisited: &[Deck], seen: &Seen, stop: &AtomicBool) -> Vec<Weighted> {
    let parts: Vec<Vec<Weighted>> = pool().install(|| {
        unvisited
            .par_chunks(CHUNK)
            .enumerate()
            .map_init(
                || (Vec::new(), Vec::new()),
                |(moves, buf), (n, decks)| {
                    let mut out = Vec::with_capacity(decks.len() * AVG_OPTIONS);
                    if stop.load(Ordering::Relaxed) {
                        return out;
                    }
                    for (k, deck) in decks.iter().enumerate() {
                        deck.get_moves(moves);
                        for &m in moves.iter() {
                            let w = deck.summary(m, buf);
                            if !seen.contains(&w.id) {
                                out.push(Weighted { parent: (n * CHUNK + k) as u32, ..w });
                            }
                        }
                    }
                    out
                },
            )
            .collect()
    });
    parts.into_iter().flatten().collect()
}

/// Construit les positions retenues, dans l'ordre de `kept`.
fn build_kept(unvisited: &[Deck], kept: &[Weighted], store: &PileStore) -> Vec<Deck> {
    pool().install(|| kept.par_iter().map(|w| unvisited[w.parent as usize].apply(w.mv, store)).collect())
}

/// Cherche une suite de coups gagnante depuis la position de `game`.
/// `stop` permet d'interrompre la recherche depuis un autre fil.
pub fn solve(game: &Game, stop: &AtomicBool) -> Outcome {
    if game.is_won() {
        return Outcome::Winnable(Vec::new());
    }

    let store = PileStore;
    let start = translate(game, &store);

    let mut unvisited: Vec<Deck> = vec![start];
    let mut seen = Seen::default();
    let max_new_unvisited = LEVEL_CAP * REDEALS_NR * AVG_OPTIONS;
    let mut discarded_siblings = false;
    let mut depth = 1;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Outcome::Aborted;
        }

        // Les positions du niveau sont développées en parallèle ; les
        // doublons sont ensuite écartés dans l'ordre de l'original, pour
        // obtenir exactement le même résultat qu'avec un seul fil.
        let children = expand_level(&unvisited, &seen, stop);
        if stop.load(Ordering::Relaxed) {
            return Outcome::Aborted;
        }
        let mut new_unvisited: Vec<Weighted> = Vec::new();
        for w in children {
            if !seen.insert(w.id) {
                continue;
            }
            new_unvisited.push(w);
            if new_unvisited.len() == max_new_unvisited {
                return Outcome::Unknown;
            }
        }

        if new_unvisited.is_empty() {
            break;
        }

        // L'original trie tout, puis parcourt dans l'ordre : il s'arrête à la
        // première position gagnante, et garde les `LEVEL_CAP` premières de
        // chaque nombre de rangées restantes. On obtient la même chose sans
        // tout trier : la meilleure position gagnante, puis les meilleures de
        // chaque groupe (sélection partielle), triées entre elles.
        if std::env::var("SOLVER_DEBUG").is_ok()
            && let Some(best) = new_unvisited.iter().min_by_key(|w| sort_key(w))
        {
            eprintln!("DEPTH {depth} {} chaos: {} playable: {}", new_unvisited.len(), best.chaos, best.playable);
        }
        depth += 1;
        if let Some(w) = new_unvisited.iter().filter(|w| w.in_off == 104).min_by_key(|w| sort_key(w)) {
            return Outcome::Winnable(unvisited[w.parent as usize].apply(w.mv, &store).move_list());
        }
        let mut groups: [Vec<Weighted>; REDEALS_NR] = Default::default();
        for w in new_unvisited {
            groups[w.left_talons as usize].push(w);
        }
        let mut kept = Vec::with_capacity(LEVEL_CAP * REDEALS_NR);
        for mut g in groups {
            if g.len() > LEVEL_CAP {
                discarded_siblings = true;
                g.select_nth_unstable_by_key(LEVEL_CAP, sort_key);
                g.truncate(LEVEL_CAP);
            }
            kept.append(&mut g);
        }
        kept.sort_unstable_by_key(sort_key);
        // Seules les positions retenues sont construites (en parallèle aussi).
        let next = build_kept(&unvisited, &kept, &store);
        // Les anciennes positions ont été construites par les fils du pool :
        // c'est à eux de les libérer (bien plus rapide que depuis un seul fil).
        let old = std::mem::replace(&mut unvisited, next);
        pool().install(|| old.into_par_iter().for_each(drop));
    }

    if !discarded_siblings { Outcome::NotWinnable } else { Outcome::Unknown }
}

/// Joue la suite de coups du solveur sur une copie du jeu : sert aux tests
/// et à vérifier qu'une solution est bien jouable.
#[cfg(test)]
pub fn replay(game: &Game, moves: &[SolverMove]) -> Option<Game> {
    let mut g = game.clone();
    for m in moves {
        if let Some(gm) = m.to_game_move() {
            if !g.play(gm) {
                return None;
            }
        }
    }
    Some(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sort_key` trie comme `compare` (l'ordre de l'original).
    #[test]
    fn sort_key_matches_compare() {
        let store = PileStore;
        let mut buf = Vec::new();
        let mut moves = Vec::new();
        let mut all = Vec::new();
        for (suits, deal) in [(1, 3), (2, 5), (4, 8)] {
            let mut deck = translate(&Game::new(suits, deal, false), &store);
            for _ in 0..40 {
                deck.get_moves(&mut moves);
                let Some(&last) = moves.last() else { break };
                all.extend(moves.iter().map(|&m| deck.summary(m, &mut buf)));
                deck = deck.apply(last, &store);
            }
        }
        all.sort_by_key(|w| w.id);
        all.dedup_by_key(|w| w.id);
        for a in &all {
            for b in all.iter().take(300) {
                assert_eq!(compare(a, b), sort_key(a).cmp(&sort_key(b)));
            }
        }
    }

    /// Le classement calculé sans construire la position est celui de la
    /// position construite.
    #[test]
    fn summary_matches_built_position() {
        let store = PileStore;
        let mut buf = Vec::new();
        let mut moves = Vec::new();
        let mut checked = 0;
        for (suits, deal) in [(1, 3), (2, 5), (4, 8)] {
            let mut deck = translate(&Game::new(suits, deal, false), &store);
            for _ in 0..60 {
                deck.get_moves(&mut moves);
                let Some(&last) = moves.last() else { break };
                for &m in &moves {
                    let s = deck.summary(m, &mut buf);
                    let built = deck.apply(m, &store).summarize_self();
                    assert_eq!(
                        (s.id, s.chaos, s.playable, s.free_plays, s.in_off, s.left_talons),
                        (built.id, built.chaos, built.playable, built.free_plays, built.in_off, built.left_talons)
                    );
                    checked += 1;
                }
                deck = deck.apply(last, &store);
            }
        }
        assert!(checked > 200, "{checked} coups vérifiés");
    }

    #[test]
    fn one_suit_deals_are_solved_and_replayable() {
        let stop = AtomicBool::new(false);
        let mut solved = 0;
        for deal in 1..=5 {
            let g = Game::new(1, deal, false);
            if let Outcome::Winnable(moves) = solve(&g, &stop) {
                let end = replay(&g, &moves).expect("solution jouable");
                assert!(end.is_won(), "donne {deal} : la solution ne gagne pas");
                solved += 1;
            }
        }
        assert!(solved >= 4, "seulement {solved}/5 donnes à 1 couleur résolues");
    }
}
