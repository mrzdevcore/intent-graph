//! The screens surface's tools, gestures and keys (the approved design's table in
//! `screen_tool`), its commands (rename, state variants, image import) and the fit of both
//! surfaces. Every edit is `ig_design` ops through `Engine::ops`: one undo step each.

use crate::engine::{EditRequest, Engine, Out, Surface};
use crate::hit::Target;
use crate::input::{Gesture, Input, Tool};
use crate::screen_scene::{label_rect, screen_rect};
use crate::screens::{nav_key, piece_key, split_key, split_nav};
use crate::{Step, snap};
use ig_design::{
    Background, ElementPatch, MAX_POINTS, Op, Piece, PieceKind, PiecePatch, Screen, ScreenPatch,
    new_piece_id, piece_size,
};

/// The screens' keys (without Cmd/Ctrl); the shared ones are the architecture board's.
pub(crate) fn screen_tool(key: &str) -> Option<Tool> {
    use PieceKind::*;
    Some(match key {
        "v" => Tool::Select,
        "s" => Tool::Screen,
        "u" => Tool::Pieces,
        "b" => Tool::Piece(Button),
        "n" => Tool::Piece(Input),
        "o" => Tool::Piece(Select),
        "c" => Tool::Piece(Checkbox),
        "t" => Tool::Piece(Table),
        "l" => Tool::Piece(List),
        "d" => Tool::Piece(Card),
        "j" => Tool::Piece(Tabs),
        "m" => Tool::Piece(Modal),
        "h" => Tool::Piece(Heading),
        "g" => Tool::Piece(Image),
        "y" => Tool::Piece(Icon),
        "p" => Tool::Pen,
        "a" => Tool::Arrow,
        "x" => Tool::Piece(Text),
        "i" => Tool::Import,
        "k" => Tool::Bridge,
        _ => return None,
    })
}

fn slug(s: &str) -> String {
    (s.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric()))
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join("-")
}

/// `base`, or `base-2`, `base-3`… when taken; short enough for a file name.
fn fresh(taken: impl Fn(&str) -> bool, base: &str) -> String {
    let base: String = base.chars().take(56).collect::<String>();
    let base = base.trim_end_matches('-');
    if !taken(base) {
        return base.into();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|i| !taken(i))
        .unwrap()
}

/// Where a corner may go so a piece of `size` (negative: it runs back) stays in `span`.
fn clamp_in(v: i32, size: i32, span: i32) -> i32 {
    let lo = -size.min(0);
    v.clamp(lo, (span - size.max(0)).max(lo))
}

fn kind_label(k: PieceKind) -> String {
    let v = serde_json::to_value(k).unwrap();
    let mut s = v.as_str().unwrap_or_default().to_string();
    s[..1].make_ascii_uppercase();
    s
}

fn inside(r: [f32; 4], w: (f32, f32)) -> bool {
    w.0 >= r[0] && w.0 <= r[0] + r[2] && w.1 >= r[1] && w.1 <= r[1] + r[3]
}

/// A copy of `p` in `screen` as `id` at `(x, y)`; `ends` keeps its arrow ends.
fn copy_ops(screen: &str, id: &str, p: &Piece, x: i32, y: i32, ends: bool) -> [Op; 2] {
    let props = match serde_json::to_value(&p.props) {
        Ok(serde_json::Value::Object(m)) if !m.is_empty() => Some(m),
        _ => None,
    };
    let end = |e: &Option<String>| e.clone().filter(|_| ends);
    [
        Op::ScreenAdd {
            screen: screen.into(),
            id: Some(id.into()),
            kind: p.kind,
            label: p.label.clone(),
            props,
            x: Some(x),
            y: Some(y),
        },
        Op::ScreenUpdate {
            screen: screen.into(),
            id: id.into(),
            set: PiecePatch {
                w: Some(p.w),
                h: Some(p.h),
                points: p.points.clone(),
                from: end(&p.from),
                to: end(&p.to),
                ..Default::default()
            },
        },
    ]
}

fn err(mut o: Out, e: &str) -> Out {
    o.error = Some(e.into());
    o
}

impl Engine {
    pub(crate) fn screen_input(&mut self, i: Input) -> Out {
        match i {
            Input::Down { x, y, button, mods } => self.screen_down(x, y, button, mods),
            Input::Move { x, y, .. } => self.screen_move(x, y),
            Input::Up { x, y, mods } => self.screen_up(x, y, mods),
            Input::Key { key, mods } => self.screen_key(&key, mods),
            _ => self.out(),
        }
    }

    /// The screen of a screen id, piece key or nav key.
    fn screen_of(&self, key: &str) -> Option<&Screen> {
        let id = (split_nav(key).map(|n| n.0))
            .or(split_key(key).map(|k| k.0))
            .unwrap_or(key);
        self.board().screens.get(id)
    }

    fn screen_at(&self, w: (f32, f32)) -> Option<String> {
        (self.board().screens.values().rev())
            .find(|s| inside(screen_rect(s), w))
            .map(|s| s.id.clone())
    }

    /// `(screen, piece)` of the topmost piece at a world point.
    fn piece_at(&self, w: (f32, f32)) -> Option<(String, String)> {
        match self.index.at(w.0, w.1, 0.0)? {
            Target::El(k) => split_key(&k).map(|(s, p)| (s.to_string(), p.to_string())),
            _ => None,
        }
    }

    /// Selected pieces and screens: what drags and nudges move.
    pub(crate) fn selected_movable(&self) -> Vec<String> {
        (self.selection.iter())
            .filter(|k| split_nav(k).is_none())
            .cloned()
            .collect()
    }

    fn screen_down(&mut self, x: f32, y: f32, button: u8, mods: u8) -> Out {
        let w = self.camera.to_world(x, y);
        match self.tool {
            _ if button == 1 => self.down(x, y, button, mods),
            Tool::Screen => self.add_screen_at(w),
            Tool::Piece(k) => self.place_piece(k, w),
            Tool::Pen => match self.screen_at(w) {
                Some(screen) => {
                    let points = vec![[w.0, w.1, self.pressure]];
                    self.gesture = Gesture::Pen { screen, points };
                    self.redraw()
                }
                None => err(self.out(), "Draw inside a screen (S draws one)"),
            },
            Tool::Arrow => {
                self.gesture = Gesture::Arrow { start: w, to: w };
                self.redraw()
            }
            Tool::Select => self.down(x, y, button, mods),
            _ => self.out(), // the palette, import and bridge tools are the host's
        }
    }

    fn screen_move(&mut self, x: f32, y: f32) -> Out {
        let w = self.camera.to_world(x, y);
        let (zoom, pressure) = (self.camera.zoom, self.pressure);
        match &mut self.gesture {
            Gesture::Pen { points, .. } => {
                let l = points[points.len() - 1];
                if (l[0] - w.0).hypot(l[1] - w.1) * zoom >= 1.0 {
                    points.push([w.0, w.1, pressure]);
                }
                self.redraw()
            }
            Gesture::Arrow { to, .. } => {
                *to = w;
                self.redraw()
            }
            _ => self.drag_to(x, y),
        }
    }

    fn screen_up(&mut self, x: f32, y: f32, mods: u8) -> Out {
        let mut o = match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Drag { ids, dx, dy, .. } => {
                let ops = self.move_ops(&ids, dx, dy);
                self.ops(&ops, Step::User)
            }
            Gesture::Pen { screen, points } => self.add_pen(&screen, &points),
            Gesture::Arrow { start, .. } => self.add_arrow(start, self.camera.to_world(x, y)),
            g => {
                self.gesture = g;
                return self.up(x, y, mods);
            }
        };
        o.redraw = true;
        o
    }

    /// Pieces snap to the grid and stay inside their screen; screens snap; a piece moves with
    /// its screen when both are selected.
    fn move_ops(&self, ids: &[String], dx: f32, dy: f32) -> Vec<Op> {
        let b = self.board();
        let mut ops = vec![];
        for id in ids {
            if let Some((sid, pid)) = split_key(id) {
                let Some(s) = b.screens.get(sid).filter(|_| !ids.iter().any(|i| i == sid)) else {
                    continue;
                };
                let Some(p) = s.elements.iter().find(|p| p.id == pid) else {
                    continue;
                };
                ops.push(Op::ScreenMove {
                    screen: sid.into(),
                    id: pid.into(),
                    x: clamp_in(snap(p.x as f32 + dx), p.w, s.w),
                    y: clamp_in(snap(p.y as f32 + dy), p.h, s.h),
                });
            } else if let Some(s) = b.screens.get(id) {
                let set = ScreenPatch {
                    x: Some(snap(s.x as f32 + dx)),
                    y: Some(snap(s.y as f32 + dy)),
                    ..Default::default()
                };
                ops.push(Op::ScreenSet {
                    id: id.clone(),
                    set,
                });
            }
        }
        ops
    }

    /// Runs `ops` as one user step; on success selects `key`, and `edit` asks for its label.
    fn made(&mut self, ops: &[Op], key: String, edit: bool) -> Out {
        let mut o = self.ops(ops, Step::User);
        if o.error.is_none() {
            o.edit = self.screen_edit(&key).filter(|_| edit);
            self.selection = vec![key];
            o.selection = self.selection.clone();
        }
        o
    }

    fn add_screen_at(&mut self, w: (f32, f32)) -> Out {
        self.tool = Tool::Select;
        let sc = &self.board().screens;
        let id = fresh(|i| sc.contains_key(i), "screen");
        let ops = [
            Op::AddScreen {
                id: Some(id.clone()),
                title: format!("Screen {}", sc.len() + 1),
                variant_of: None,
                state: None,
            },
            Op::ScreenSet {
                id: id.clone(),
                set: ScreenPatch {
                    x: Some(snap(w.0)),
                    y: Some(snap(w.1)),
                    ..Default::default()
                },
            },
        ];
        self.made(&ops, id, true)
    }

    fn place_piece(&mut self, k: PieceKind, w: (f32, f32)) -> Out {
        let Some(s) = self
            .screen_at(w)
            .map(|id| self.board().screens[&id].clone())
        else {
            return err(self.out(), "Pieces go inside a screen (S draws one)");
        };
        self.tool = Tool::Select;
        let (pw, ph) = piece_size(k);
        let x = clamp_in(snap(w.0 - s.x as f32 - pw as f32 / 2.0), pw, s.w);
        let y = clamp_in(snap(w.1 - s.y as f32 - ph as f32 / 2.0), ph, s.h);
        let label = kind_label(k);
        let id = new_piece_id(&s, k, &label);
        let op = Op::ScreenAdd {
            screen: s.id.clone(),
            id: Some(id.clone()),
            kind: k,
            label,
            props: None,
            x: Some(x),
            y: Some(y),
        };
        self.made(&[op], piece_key(&s.id, &id), true)
    }

    /// One stroke, one step: points in the piece's px, pressure 0-100, at most `MAX_POINTS`.
    fn add_pen(&mut self, screen: &str, points: &[[f32; 3]]) -> Out {
        let Some(s) = self.board().screens.get(screen).cloned() else {
            return self.out();
        };
        if points.len() < 2 {
            return self.out();
        }
        let every = points.len().div_ceil(MAX_POINTS);
        let local: Vec<[f32; 3]> = (points.iter().step_by(every))
            .map(|p| {
                let x = (p[0] - s.x as f32).clamp(0.0, s.w as f32);
                [x, (p[1] - s.y as f32).clamp(0.0, s.h as f32), p[2]]
            })
            .collect();
        let lo = |i: usize| local.iter().map(|p| p[i]).fold(f32::MAX, f32::min).floor() as i32;
        let hi = |i: usize| local.iter().map(|p| p[i]).fold(f32::MIN, f32::max).ceil() as i32;
        let (x, y) = (lo(0), lo(1));
        let pts = (local.iter())
            .map(|p| {
                let q = (p[2] * 100.0).round().clamp(0.0, 100.0) as i32;
                [p[0].round() as i32 - x, p[1].round() as i32 - y, q]
            })
            .collect();
        let id = new_piece_id(&s, PieceKind::Pen, "");
        let pen = Piece {
            id: id.clone(),
            kind: PieceKind::Pen,
            label: String::new(),
            props: Default::default(),
            x,
            y,
            w: (hi(0) - x).max(1),
            h: (hi(1) - y).max(1),
            points: Some(pts),
            from: None,
            to: None,
        };
        let ops = copy_ops(screen, &id, &pen, x, y, false);
        self.made(&ops, piece_key(screen, &id), false)
    }

    /// From a piece onto another screen's frame: a nav, its label edited in place. Else an
    /// arrow in the screen it starts in, tied to the pieces at its ends.
    fn add_arrow(&mut self, start: (f32, f32), end: (f32, f32)) -> Out {
        if (end.0 - start.0).hypot(end.1 - start.1) * self.camera.zoom < 4.0 {
            return self.out();
        }
        let from = self.piece_at(start);
        let target = self.screen_at(end);
        if let (Some((fs, fp)), Some(ts)) = (&from, &target)
            && fs != ts
        {
            let op = Op::Nav {
                screen: fs.clone(),
                from: fp.clone(),
                to: ts.clone(),
                label: String::new(),
            };
            let n = ig_design::Nav {
                from: fp.clone(),
                to: ts.clone(),
                label: String::new(),
            };
            return self.made(&[op], nav_key(fs, &n), true);
        }
        let Some(s) = self
            .screen_at(start)
            .map(|id| self.board().screens[&id].clone())
        else {
            return err(self.out(), "Arrows start inside a screen");
        };
        let local = |w: (f32, f32)| {
            let x = snap(w.0 - s.x as f32).clamp(0, s.w);
            (x, snap(w.1 - s.y as f32).clamp(0, s.h))
        };
        let ((x, y), (ex, ey)) = (local(start), local(end));
        if (x, y) == (ex, ey) {
            return self.out();
        }
        let on = |p: Option<(String, String)>| p.filter(|p| p.0 == s.id).map(|p| p.1);
        let f = on(from);
        let t = on(self.piece_at(end)).filter(|t| Some(t) != f.as_ref());
        let id = new_piece_id(&s, PieceKind::Arrow, "");
        let arrow = Piece {
            id: id.clone(),
            kind: PieceKind::Arrow,
            label: String::new(),
            props: Default::default(),
            x,
            y,
            w: ex - x,
            h: ey - y,
            points: None,
            from: f,
            to: t,
        };
        let ops = copy_ops(&s.id, &id, &arrow, x, y, true);
        self.made(&ops, piece_key(&s.id, &id), false)
    }

    /// A state of `screen` (loading, error, empty…): a copy below its base and the base's
    /// other states, same pieces (new keys: `<new screen>/<piece>`), navs and background.
    pub fn variant(&mut self, screen: &str, state: &str) -> Out {
        let state = state.trim();
        let b = self.board();
        let Some(src) = b.screens.get(screen).cloned().filter(|_| !state.is_empty()) else {
            return err(self.out(), "A state needs a screen and a name");
        };
        let base_id = src.variant_of.clone().unwrap_or_else(|| src.id.clone());
        let base = b.screens.get(&base_id).unwrap_or(&src);
        let id = fresh(
            |i| b.screens.contains_key(i),
            &format!("{base_id}-{}", slug(state)),
        );
        let below = (b.screens.values())
            .filter(|s| s.id == base_id || s.variant_of.as_deref() == Some(&base_id))
            .map(|s| s.y + s.h)
            .max()
            .unwrap_or(src.y + src.h);
        let mut ops = vec![
            Op::AddScreen {
                id: Some(id.clone()),
                title: base.title.clone(), // the state shows as a chip by the name
                variant_of: Some(base_id.clone()),
                state: Some(state.into()),
            },
            Op::ScreenSet {
                id: id.clone(),
                set: ScreenPatch {
                    x: Some(base.x),
                    y: Some(below + 80),
                    w: Some(src.w),
                    h: Some(src.h),
                    background: Some(src.background.clone()),
                    ..Default::default()
                },
            },
        ];
        for p in &src.elements {
            ops.extend(copy_ops(&id, &p.id, p, p.x, p.y, true));
        }
        ops.extend(src.navs.iter().map(|n| Op::Nav {
            screen: id.clone(),
            from: n.from.clone(),
            to: n.to.clone(),
            label: n.label.clone(),
        }));
        self.made(&ops, id, false)
    }

    /// The host stored and decoded `asset`: it becomes the locked background of the selected
    /// screen, else the one in the middle of the view, else of a new screen its size.
    pub fn import_image(&mut self, asset: &str) -> Out {
        let size = self.images.get(asset).map(|&(w, h)| (w as i32, h as i32));
        let c = self.camera.to_world(self.view[2] / 2.0, self.view[3] / 2.0);
        let target = (self.selection.iter())
            .find_map(|k| self.screen_of(k).map(|s| s.id.clone()))
            .or_else(|| self.screen_at(c));
        let sc = &self.board().screens;
        let fit =
            |w: i32| size.map(|(iw, ih)| (w, (w as i64 * ih as i64 / iw.max(1) as i64) as i32));
        let mut ops = vec![];
        let (id, (w, h)) = match target.and_then(|t| sc.get(&t)) {
            Some(s) => (s.id.clone(), fit(s.w).unwrap_or((s.w, s.h))),
            None => {
                let id = fresh(|i| sc.contains_key(i), "screen");
                let (w, h) = fit(size.map_or(1280, |s| s.0.min(1280))).unwrap_or((1280, 800));
                ops.push(Op::AddScreen {
                    id: Some(id.clone()),
                    title: "Imported".into(),
                    variant_of: None,
                    state: None,
                });
                let at = ScreenPatch {
                    x: Some(snap(c.0 - w as f32 / 2.0)),
                    y: Some(snap(c.1 - h as f32 / 2.0)),
                    w: Some(w),
                    h: Some(h),
                    ..Default::default()
                };
                ops.push(Op::ScreenSet {
                    id: id.clone(),
                    set: at,
                });
                (id, (w, h))
            }
        };
        let bg = Background {
            asset: asset.into(),
            x: 0,
            y: 0,
            w,
            h,
        };
        let set = ScreenPatch {
            background: Some(Some(bg)),
            ..Default::default()
        };
        ops.push(Op::ScreenSet {
            id: id.clone(),
            set,
        });
        self.made(&ops, id, false)
    }

    /// The rename overlay's commit: an element's name, a screen's title, a piece's or nav's label.
    pub fn rename(&mut self, id: &str, text: &str) -> Out {
        let op = match (self.surface, split_nav(id), split_key(id)) {
            (Surface::Architecture, ..) => Op::Update {
                id: id.into(),
                set: ElementPatch {
                    name: Some(text.into()),
                    ..Default::default()
                },
            },
            (_, Some((s, f, t)), _) => Op::Nav {
                screen: s.into(),
                from: f.into(),
                to: t.into(),
                label: text.into(),
            },
            (_, _, Some((s, p))) => Op::ScreenUpdate {
                screen: s.into(),
                id: p.into(),
                set: PiecePatch {
                    label: Some(text.into()),
                    ..Default::default()
                },
            },
            _ => Op::ScreenSet {
                id: id.into(),
                set: ScreenPatch {
                    title: Some(text.into()),
                    ..Default::default()
                },
            },
        };
        self.ops(&[op], Step::User)
    }

    pub(crate) fn screen_edit(&self, key: &str) -> Option<EditRequest> {
        let b = self.board();
        let r = label_rect(b, key)?;
        let text = match (split_nav(key), split_key(key)) {
            (Some((s, f, t)), _) => (b.screens.get(s)?.navs.iter())
                .find(|n| n.from == f && n.to == t)?
                .label
                .clone(),
            (_, Some((s, p))) => (b.screens.get(s)?.elements.iter())
                .find(|e| e.id == p)?
                .label
                .clone(),
            _ => b.screens.get(key)?.title.clone(),
        };
        Some(EditRequest {
            id: key.into(),
            rect: self.camera.to_screen(r),
            text,
        })
    }
}

#[path = "screen_keys.rs"]
mod keys;

#[cfg(test)]
#[path = "screen_tests.rs"]
mod tests;
