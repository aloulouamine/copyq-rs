//! Petites icônes vectorielles dessinées à la main (traits fins, style « SF Symbols »).
//! Les glyphes emoji d'egui sont trop petits et de styles disparates à cette taille.

use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Rounding, Shape, Stroke};

const W: f32 = 1.4;

fn s(color: Color32) -> Stroke {
    Stroke::new(W, color)
}

/// Globe (liens web).
pub fn globe(p: &Painter, c: Pos2, color: Color32) {
    let r = 7.0;
    p.circle_stroke(c, r, s(color));
    p.line_segment([pos2(c.x - r, c.y), pos2(c.x + r, c.y)], s(color));
    // Méridien central : ellipse approchée par une polyligne.
    let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0 * std::f32::consts::TAU;
            pos2(c.x + 3.2 * t.cos(), c.y + r * t.sin())
        })
        .collect();
    p.add(Shape::line(pts, s(color)));
}

/// Dossier (chemins de fichiers).
pub fn folder(p: &Painter, c: Pos2, color: Color32) {
    let body = Rect::from_center_size(pos2(c.x, c.y + 1.0), vec2(15.0, 10.5));
    p.rect_stroke(body, Rounding::same(2.0), s(color));
    let tab = vec![
        pos2(body.left() + 0.5, body.top()),
        pos2(body.left() + 0.5, body.top() - 2.5),
        pos2(body.left() + 5.5, body.top() - 2.5),
        pos2(body.left() + 7.5, body.top()),
    ];
    p.add(Shape::line(tab, s(color)));
}

/// Enveloppe (adresses e-mail).
pub fn envelope(p: &Painter, c: Pos2, color: Color32) {
    let body = Rect::from_center_size(c, vec2(15.0, 11.0));
    p.rect_stroke(body, Rounding::same(2.0), s(color));
    p.add(Shape::line(
        vec![
            pos2(body.left() + 1.0, body.top() + 1.5),
            pos2(c.x, c.y + 1.5),
            pos2(body.right() - 1.0, body.top() + 1.5),
        ],
        s(color),
    ));
}

/// Lignes de texte (contenu sur plusieurs lignes / code).
pub fn lines(p: &Painter, c: Pos2, color: Color32) {
    for (dy, w) in [(-4.5, 13.0), (0.0, 9.0), (4.5, 11.0)] {
        let x0 = c.x - 6.5;
        p.line_segment([pos2(x0, c.y + dy), pos2(x0 + w, c.y + dy)], s(color));
    }
}

/// Flèche « Entrée » ⏎.
pub fn enter(p: &Painter, c: Pos2, color: Color32) {
    let st = Stroke::new(1.6_f32, color);
    let right = c.x + 5.0;
    let left = c.x - 5.0;
    let top = c.y - 5.0;
    let base = c.y + 2.5;
    p.add(Shape::line(
        vec![pos2(right, top), pos2(right, base), pos2(left, base)],
        st,
    ));
    p.add(Shape::line(
        vec![
            pos2(left + 3.5, base - 3.5),
            pos2(left, base),
            pos2(left + 3.5, base + 3.5),
        ],
        st,
    ));
}

/// Corbeille.
pub fn trash(p: &Painter, c: Pos2, color: Color32) {
    let st = s(color);
    let lid_y = c.y - 4.5;
    p.line_segment([pos2(c.x - 6.0, lid_y), pos2(c.x + 6.0, lid_y)], st);
    p.add(Shape::line(
        vec![
            pos2(c.x - 2.5, lid_y),
            pos2(c.x - 2.0, lid_y - 2.0),
            pos2(c.x + 2.0, lid_y - 2.0),
            pos2(c.x + 2.5, lid_y),
        ],
        st,
    ));
    p.add(Shape::line(
        vec![
            pos2(c.x - 4.8, lid_y),
            pos2(c.x - 4.0, c.y + 6.0),
            pos2(c.x + 4.0, c.y + 6.0),
            pos2(c.x + 4.8, lid_y),
        ],
        st,
    ));
    for dx in [-1.6, 1.6] {
        p.line_segment([pos2(c.x + dx, c.y - 1.5), pos2(c.x + dx, c.y + 3.5)], st);
    }
}

/// Cadenas (mots de passe enregistrés, affichés masqués).
pub fn lock(p: &Painter, c: Pos2, color: Color32) {
    let body = Rect::from_center_size(pos2(c.x, c.y + 2.5), vec2(12.0, 9.0));
    p.rect_stroke(body, Rounding::same(2.0), s(color));
    let r = 3.5;
    let base = body.top();
    let arc: Vec<Pos2> = (0..=12)
        .map(|i| {
            let t = std::f32::consts::PI * (i as f32 / 12.0);
            pos2(c.x - r * t.cos(), base - 2.0 - r * t.sin())
        })
        .collect();
    let mut shackle = vec![pos2(c.x - r, base)];
    shackle.extend(arc);
    shackle.push(pos2(c.x + r, base));
    p.add(Shape::line(shackle, s(color)));
    p.circle_filled(pos2(c.x, c.y + 2.5), 1.3, color);
}
