//! Cartes, paquet et mélange.
//!
//! Le paquet est construit et mélangé exactement comme dans KPatience, pour
//! qu'un même numéro de donne donne la même partie dans les deux jeux (et
//! dans Packarch Spider, la version C++).

use serde::{Deserialize, Serialize};

/// Les couleurs, dans l'ordre (et avec les valeurs) de KPatience.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Suit {
    Clubs = 0,
    Diamonds = 1,
    Hearts = 2,
    Spades = 3,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    /// Cœur et carreau sont rouges.
    pub fn is_red(self) -> bool {
        matches!(self, Suit::Diamonds | Suit::Hearts)
    }

    /// Nom de la couleur dans les éléments des jeux de cartes SVG.
    pub fn svg_name(self) -> &'static str {
        match self {
            Suit::Clubs => "club",
            Suit::Diamonds => "diamond",
            Suit::Hearts => "heart",
            Suit::Spades => "spade",
        }
    }
}

/// Une carte. `id` est unique dans le paquet (0..104 au Spider, 0..156 au
/// FreeCell à 3 paquets) et sert à suivre la
/// carte pendant les animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: u8,
    /// 1 (As) à 13 (Roi).
    pub rank: u8,
    pub suit: Suit,
    pub face_up: bool,
}

impl Card {
    /// Nom de l'élément de la face dans les jeux de cartes SVG (« 1_spade »,
    /// « queen_heart »…).
    pub fn svg_name(&self) -> String {
        svg_face_name(self.rank, self.suit)
    }
}

pub fn svg_face_name(rank: u8, suit: Suit) -> String {
    let rank = match rank {
        11 => "jack".to_string(),
        12 => "queen".to_string(),
        13 => "king".to_string(),
        r => r.to_string(),
    };
    format!("{rank}_{}", suit.svg_name())
}

/// Les couleurs utilisées selon le nombre de couleurs de la partie. Comme
/// dans KPatience, on passe toujours 4 couleurs (éventuellement répétées)
/// pour ne pas changer la numérotation des donnes.
pub fn suits_for(count: u8) -> [Suit; 4] {
    use Suit::*;
    match count {
        1 => [Spades, Spades, Spades, Spades],
        2 => [Hearts, Spades, Hearts, Spades],
        3 => [Clubs, Spades, Hearts, Spades],
        _ => [Clubs, Diamonds, Hearts, Spades],
    }
}

/// Un paquet de `copies` jeux, dans l'ordre de création de KPatience
/// (`DealerScene::setDeckContents`) : pour chaque copie, rang par rang, puis
/// couleur par couleur. Cartes face cachée.
pub fn deck(copies: u8, suits: &[Suit]) -> Vec<Card> {
    let mut cards = Vec::with_capacity(copies as usize * 13 * suits.len());
    let mut id = 0u8;
    for _copy in 0..copies {
        for rank in 1..=13u8 {
            for &suit in suits {
                cards.push(Card { id, rank, suit, face_up: false });
                id += 1;
            }
        }
    }
    cards
}

/// Le paquet de 104 cartes du Spider (2 copies).
pub fn spider_deck(suit_count: u8) -> Vec<Card> {
    deck(2, &suits_for(suit_count))
}

/// Mélange de KPatience : le générateur pseudo-aléatoire du FreeCell de
/// Windows, pour que les numéros de donne soient les mêmes.
pub fn shuffled<T: Clone>(cards: &[T], seed: u32) -> Vec<T> {
    let mut result = cards.to_vec();
    let mut seed = seed;
    let mut i = result.len();
    while i > 1 {
        seed = seed.wrapping_mul(214013).wrapping_add(2531011);
        let rand = ((seed >> 16) & 0x7fff) as usize;
        result.swap(i - 1, rand % i);
        i -= 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_has_104_unique_cards() {
        for suits in 1..=4 {
            let deck = spider_deck(suits);
            assert_eq!(deck.len(), 104);
            let mut ids: Vec<u8> = deck.iter().map(|c| c.id).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), 104);
        }
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let deck = spider_deck(4);
        let s = shuffled(&deck, 12345);
        let mut ids: Vec<u8> = s.iter().map(|c| c.id).collect();
        ids.sort();
        assert_eq!(ids, (0..104).collect::<Vec<u8>>());
    }

    #[test]
    fn svg_names() {
        assert_eq!(svg_face_name(1, Suit::Spades), "1_spade");
        assert_eq!(svg_face_name(12, Suit::Hearts), "queen_heart");
    }
}
