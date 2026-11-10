//! Jeux de cartes et thèmes SVG (ceux de KDE / KPatience).
//!
//! Un jeu de cartes est un dossier `carddecks/<nom>/` avec un `index.desktop`
//! (`[KDE Backdeck]`, clés `Name`, `SVG`, `Features`) et le SVG, dont les
//! éléments s'appellent `back`, `1_spade`, `queen_heart`… Un thème est un
//! `themes/<nom>.desktop` (`[KGameTheme]`, `Name`, `FileName`) dont le SVG
//! contient `background`, `pile`, `message_frame`…
//!
//! Le jeu de cartes et le thème par défaut sont intégrés au binaire.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use eframe::egui::ColorImage;
use resvg::{tiny_skia, usvg};

pub const DEFAULT_DECK: &str = "svg-oxygen-air";
pub const DEFAULT_THEME: &str = "greenblaze";

const EMBEDDED_DECK: &[u8] = include_bytes!("../data/carddecks/svg-oxygen-air/oxygen-air.svg");
const EMBEDDED_THEME: &[u8] = include_bytes!("../data/themes/greenblaze.svg");

#[derive(Clone, Debug)]
pub struct AssetInfo {
    /// Identifiant (nom du dossier du jeu de cartes, nom du thème).
    pub id: String,
    pub name: String,
    /// `None` : version intégrée au binaire.
    pub svg: Option<PathBuf>,
}

/// Dossiers où chercher les données : à côté du binaire installé
/// (`<préfixe>/share/packarch-spider`), dans les dossiers de données XDG,
/// et `data/` des sources (pour `cargo run`).
fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin) = exe.parent() {
            dirs.push(bin.join("../share/packarch-spider"));
        }
    }
    dirs.push(crate::storage::data_dir());
    let xdg = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in xdg.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(d).join("packarch-spider"));
    }
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/data")));
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// Lecture minimale d'un fichier `.desktop` : clé → valeur d'un groupe,
/// avec la traduction française si elle existe.
fn desktop_group(path: &Path, group: &str) -> Option<HashMap<String, String>> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_group = false;
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == format!("[{group}]");
            continue;
        }
        if !in_group {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            values.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    if values.is_empty() { None } else { Some(values) }
}

fn localized(values: &HashMap<String, String>, key: &str) -> Option<String> {
    values.get(&format!("{key}[fr]")).or_else(|| values.get(key)).cloned()
}

/// `index.desktop` référence souvent `xxx.svgz` alors que les sources
/// fournissent `xxx.svg` : on accepte les deux.
fn existing_svg(dir: &Path, name: &str) -> Option<PathBuf> {
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    let alt = dir.join(name.strip_suffix('z')?);
    alt.is_file().then_some(alt)
}

pub fn find_decks() -> Vec<AssetInfo> {
    let mut decks = vec![AssetInfo { id: DEFAULT_DECK.into(), name: "Oxygen Air".into(), svg: None }];
    let mut roots: Vec<PathBuf> = data_dirs().into_iter().map(|d| d.join("carddecks")).collect();
    // Les jeux de cartes de KDE (libkdegames), s'ils sont installés.
    roots.push(PathBuf::from("/usr/share/carddecks"));
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for dir in entries {
            let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
            let Some(values) = desktop_group(&dir.join("index.desktop"), "KDE Backdeck") else { continue };
            let features = values.get("Features").cloned().unwrap_or_else(|| "AngloAmerican".into());
            if !features.contains("AngloAmerican") {
                continue;
            }
            let Some(svg) = values.get("SVG").and_then(|s| existing_svg(&dir, s)) else { continue };
            let name = localized(&values, "Name").unwrap_or_else(|| id.clone());
            match decks.iter_mut().find(|d| d.id == id) {
                // Version sur disque du jeu par défaut : on garde le nom, la version intégrée suffit.
                Some(d) if d.svg.is_none() => d.name = name,
                Some(_) => {}
                None => decks.push(AssetInfo { id, name, svg: Some(svg) }),
            }
        }
    }
    decks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    decks
}

pub fn find_themes() -> Vec<AssetInfo> {
    let mut themes = vec![AssetInfo { id: DEFAULT_THEME.into(), name: "Vert flamboyant".into(), svg: None }];
    for root in data_dirs().into_iter().map(|d| d.join("themes")) {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for file in entries {
            if file.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = file.file_stem().and_then(|n| n.to_str()).map(String::from) else { continue };
            let Some(values) = desktop_group(&file, "KGameTheme") else { continue };
            let Some(svg) = values.get("FileName").and_then(|s| existing_svg(&root, s)) else { continue };
            let name = localized(&values, "Name").unwrap_or_else(|| id.clone());
            match themes.iter_mut().find(|t| t.id == id) {
                Some(t) if t.svg.is_none() => t.name = name,
                Some(_) => {}
                None => themes.push(AssetInfo { id, name, svg: Some(svg) }),
            }
        }
    }
    themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    themes
}

fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Un SVG chargé, dont on rend des éléments à la demande.
pub struct Svg {
    tree: usvg::Tree,
}

impl Svg {
    pub fn load(info: &AssetInfo, embedded: &[u8]) -> Result<Svg, String> {
        let data = match &info.svg {
            None => embedded.to_vec(),
            Some(path) => fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?,
        };
        let options = usvg::Options { fontdb: fontdb(), ..Default::default() };
        let tree = usvg::Tree::from_data(&data, &options).map_err(|e| e.to_string())?;
        Ok(Svg { tree })
    }

    pub fn load_deck(info: &AssetInfo) -> Result<Svg, String> {
        Svg::load(info, EMBEDDED_DECK)
    }

    pub fn load_theme(info: &AssetInfo) -> Result<Svg, String> {
        Svg::load(info, EMBEDDED_THEME)
    }

    /// Taille (largeur, hauteur) de l'élément dans le SVG.
    pub fn element_size(&self, id: &str) -> Option<(f32, f32)> {
        let node = self.tree.node_by_id(id)?;
        let b = node.abs_layer_bounding_box()?;
        Some((b.width(), b.height()))
    }

    /// Le rectangle à afficher pour l'élément, en coordonnées absolues.
    /// D'habitude sa boîte englobante ; mais certains jeux de cartes ont,
    /// dans une face, un objet parasite loin de la carte (svg-nicu-white) :
    /// si l'élément est nettement plus grand qu'un dos de carte, on cadre
    /// sur le tracé intérieur qui a la taille d'une carte.
    fn frame_of(&self, node: &usvg::Node) -> Option<usvg::NonZeroRect> {
        let bbox = node.abs_layer_bounding_box()?;
        let Some((bw, bh)) = self.element_size("back") else { return Some(bbox) };
        if bbox.width() <= bw * 1.1 && bbox.height() <= bh * 1.1 {
            return Some(bbox);
        }
        fn visit(node: &usvg::Node, out: &mut Vec<usvg::NonZeroRect>) {
            if let Some(b) = node.abs_layer_bounding_box() {
                out.push(b);
            }
            if let usvg::Node::Group(g) = node {
                for child in g.children() {
                    visit(child, out);
                }
            }
        }
        let mut rects = Vec::new();
        visit(node, &mut rects);
        let card_sized = |r: &&usvg::NonZeroRect| (r.width() / bw - 1.0).abs() < 0.1 && (r.height() / bh - 1.0).abs() < 0.1;
        let best = rects.iter().filter(card_sized).max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())));
        Some(best.copied().unwrap_or(bbox))
    }

    /// Rend l'élément `id`, étiré à `width` × `height` pixels.
    pub fn render(&self, id: &str, width: u32, height: u32) -> Option<ColorImage> {
        let node = self.tree.node_by_id(id)?;
        let layer = node.abs_layer_bounding_box()?;
        let frame = self.frame_of(node)?;
        let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))?;
        // `render_node` applique la transformation propre de l'élément mais
        // pas celles de ses parents (calques décalés…), alors que sa boîte
        // englobante est en coordonnées absolues : on ajoute celles des parents.
        let own = match node {
            usvg::Node::Group(g) => g.transform(),
            _ => tiny_skia::Transform::identity(),
        };
        let parents = own.invert().map_or(node.abs_transform(), |inv| node.abs_transform().pre_concat(inv));
        // `render_node` décale de la boîte englobante (`layer`) : on le compense.
        let transform = tiny_skia::Transform::from_scale(width as f32 / frame.width(), height as f32 / frame.height())
            .pre_translate(-frame.x(), -frame.y())
            .pre_concat(parents)
            .pre_translate(layer.x(), layer.y());
        resvg::render_node(node, transform, &mut pixmap.as_mut())?;
        Some(ColorImage::from_rgba_premultiplied([pixmap.width() as usize, pixmap.height() as usize], pixmap.data()))
    }
}

impl Svg {
    /// La couleur d'un élément (rectangle de couleur des thèmes, comme
    /// `bubble_text_color`), prise en son centre.
    pub fn element_color(&self, id: &str) -> Option<eframe::egui::Color32> {
        let img = self.render(id, 8, 8)?;
        let [r, g, b, a] = img.pixels[4 * 8 + 4].to_srgba_unmultiplied();
        (a > 0).then(|| eframe::egui::Color32::from_rgb(r, g, b))
    }
}

/// Décode une image PNG (aperçus des jeux).
pub fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    let pixmap = tiny_skia::Pixmap::decode_png(bytes).ok()?;
    Some(ColorImage::from_rgba_premultiplied([pixmap.width() as usize, pixmap.height() as usize], pixmap.data()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_deck_renders_cards() {
        let info = AssetInfo { id: DEFAULT_DECK.into(), name: String::new(), svg: None };
        let svg = Svg::load_deck(&info).unwrap();
        for id in ["back", "1_spade", "queen_heart", "10_club"] {
            let size = svg.element_size(id);
            let img = svg.render(id, 80, 112);
            let opaque = img.as_ref().map(|i| i.pixels.iter().filter(|p| p.a() > 0).count());
            eprintln!("{id}: size={size:?} opaque_pixels={opaque:?}");
            assert!(opaque.unwrap_or(0) > 80 * 112 / 2, "{id} mal rendu");
        }
    }

    /// Chaque jeu de cartes et thème fourni se charge et se rend.
    #[test]
    fn previews_decode() {
        for f in ["freecell", "spider"] {
            let bytes = fs::read(format!("data/previews/{f}.png")).unwrap();
            let img = decode_png(&bytes).expect("PNG lisible");
            assert_eq!(img.size, [480, 320]);
        }
    }

    #[test]
    fn all_decks_and_themes_render() {
        let decks = find_decks();
        assert!(decks.len() >= 7, "{} jeux de cartes trouvés", decks.len());
        for d in &decks {
            let svg = Svg::load_deck(d).unwrap_or_else(|e| panic!("{} : {e}", d.id));
            for id in ["back", "1_spade", "king_heart", "7_diamond", "jack_club"] {
                let img = svg.render(id, 60, 84).unwrap_or_else(|| panic!("{} : {id} absent", d.id));
                let opaque = img.pixels.iter().filter(|p| p.a() > 0).count();
                assert!(opaque > 60 * 84 / 2, "{} : {id} mal rendu ({opaque} pixels)", d.id);
            }
        }
        let themes = find_themes();
        assert!(themes.len() >= 4, "{} thèmes trouvés", themes.len());
        for t in &themes {
            let svg = Svg::load_theme(t).unwrap_or_else(|e| panic!("{} : {e}", t.id));
            for id in ["background", "pile", "message_frame", "bubble", "bubble_hover"] {
                let img = svg.render(id, 120, 90).unwrap_or_else(|| panic!("{} : {id} absent", t.id));
                let opaque = img.pixels.iter().filter(|p| p.a() > 0).count();
                assert!(opaque > 0, "{} : {id} vide", t.id);
            }
        }
    }
}
