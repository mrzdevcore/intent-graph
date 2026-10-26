//! The screens' keys, delete, copy and paste of pieces, and the fit of both surfaces.

use super::{clamp_in, copy_ops, screen_tool};
use crate::engine::{Engine, Out, Surface};
use crate::input::{Input, mods::*};
use crate::screen_scene::{piece_rect, screen_rect};
use crate::screens::{piece_key, split_key, split_nav};
use crate::{Step, snap};
use ig_design::{Op, new_piece_id};

impl Engine {
    pub(crate) fn screen_key(&mut self, key: &str, mods: u8) -> Out {
        let (cmd, shift) = (mods & (CTRL | META) != 0, mods & SHIFT != 0);
        let k = key.to_ascii_lowercase();
        if cmd {
            return match k.as_str() {
                "c" => {
                    self.copy_pieces();
                    self.out()
                }
                "v" => self.paste_pieces(),
                "d" => {
                    self.copy_pieces();
                    self.paste_pieces()
                }
                "a" => {
                    self.selection = (self.board().screens.values())
                        .flat_map(|s| s.elements.iter().map(|p| piece_key(&s.id, &p.id)))
                        .collect();
                    self.redraw()
                }
                _ => self.key(key, mods), // undo, redo, 100 %
            };
        }
        let nudge = if shift { 40.0 } else { 8.0 };
        let arrow = match k.as_str() {
            "arrowleft" => Some((-nudge, 0.0)),
            "arrowright" => Some((nudge, 0.0)),
            "arrowup" => Some((0.0, -nudge)),
            "arrowdown" => Some((0.0, nudge)),
            _ => None,
        };
        if let Some((dx, dy)) = arrow {
            let ops = self.move_ops(&self.selected_movable(), dx, dy);
            return self.ops(&ops, Step::User);
        }
        match k.as_str() {
            "delete" | "backspace" => self.remove_screen_selection(),
            "enter" => {
                let mut o = self.out();
                o.edit = self.selection.first().and_then(|id| self.screen_edit(id));
                o
            }
            "!" | "1" | "@" | "2" if shift => {
                self.fit(matches!(k.as_str(), "@" | "2"));
                self.redraw()
            }
            "escape" => self.key(key, mods),
            _ => match screen_tool(&k).filter(|_| !shift) {
                Some(t) => self.input(Input::Tool(t)),
                None => self.out(),
            },
        }
    }

    /// Navs first, then pieces (taking their navs and bridges), then screens, variants before
    /// their base; what a removed screen or piece already took is not removed twice.
    fn remove_screen_selection(&mut self) -> Out {
        let b = self.board();
        let sel = &self.selection;
        let gone_screen = |s: &str| sel.iter().any(|k| k == s) && b.screens.contains_key(s);
        let gone_piece = |s: &str, p: &str| sel.contains(&piece_key(s, p));
        let mut ops: Vec<Op> = vec![];
        for k in sel {
            if let Some((s, f, t)) = split_nav(k)
                && !gone_screen(s)
                && !gone_screen(t)
                && !gone_piece(s, f)
            {
                ops.push(Op::Unnav {
                    screen: s.into(),
                    from: f.into(),
                    to: t.into(),
                });
            }
        }
        for k in sel {
            if let Some((s, p)) = split_key(k).filter(|(s, _)| !gone_screen(s)) {
                ops.push(Op::ScreenRemove {
                    screen: s.into(),
                    id: p.into(),
                });
            }
        }
        let mut screens: Vec<_> = sel.iter().filter_map(|k| b.screens.get(k)).collect();
        screens.sort_by_key(|s| s.variant_of.is_none());
        ops.extend(
            screens
                .iter()
                .map(|s| Op::RemoveScreen { id: s.id.clone() }),
        );
        self.ops(&ops, Step::User)
    }

    fn copy_pieces(&mut self) {
        let b = self.board();
        self.pieces_clip = (self.selection.iter())
            .filter_map(|k| split_key(k))
            .filter_map(|(s, p)| {
                let piece = b.screens.get(s)?.elements.iter().find(|e| e.id == p)?;
                Some((s.to_string(), piece.clone()))
            })
            .collect();
        self.clipboard.2 = 0;
    }

    /// Into the screen each piece came from, 16 px further each time, with fresh ids.
    fn paste_pieces(&mut self) -> Out {
        if self.pieces_clip.is_empty() {
            return self.out();
        }
        self.clipboard.2 += 1;
        let off = self.clipboard.2 * 16;
        let mut screens = self.board().screens.clone();
        let (mut ops, mut keys) = (vec![], vec![]);
        for (sid, p) in &self.pieces_clip {
            let Some(s) = screens.get_mut(sid) else {
                continue;
            };
            let id = new_piece_id(s, p.kind, &p.label);
            let x = clamp_in(snap((p.x + off) as f32), p.w, s.w);
            let y = clamp_in(snap((p.y + off) as f32), p.h, s.h);
            ops.extend(copy_ops(sid, &id, p, x, y, false));
            keys.push(piece_key(sid, &id));
            let mut taken = p.clone();
            taken.id = id;
            s.elements.push(taken);
        }
        let mut o = self.ops(&ops, Step::User);
        if o.error.is_none() {
            self.selection = keys;
            o.selection = self.selection.clone();
        }
        o
    }

    /// Fits the view to the board and its context (the screens), or (`selected`) to the
    /// selection.
    pub(crate) fn fit(&mut self, selected: bool) {
        let sel = |id: &String| !selected || self.selection.contains(id);
        let rects: Vec<[f32; 4]> = match self.surface {
            Surface::Architecture => (self.board().design.elements.iter())
                .filter(|e| sel(&e.id))
                .filter_map(|e| self.shown_rect(e))
                .chain(
                    (self.ctx.boxes.iter())
                        .filter(|c| sel(&c.id))
                        .map(|c| [c.x, c.y, c.w, c.h]),
                )
                .collect(),
            Surface::Screens => (self.board().screens.values())
                .flat_map(|s| {
                    let pieces = (s.elements.iter())
                        .map(move |p| (piece_key(&s.id, &p.id), piece_rect(s, p, (0.0, 0.0))));
                    std::iter::once((s.id.clone(), screen_rect(s))).chain(pieces)
                })
                .filter(|(k, _)| sel(k))
                .map(|(_, r)| r)
                .collect(),
        };
        let b = rects.into_iter().fold(None, |a: Option<[f32; 4]>, r| {
            let [x0, y0, x1, y1] = a.unwrap_or([r[0], r[1], r[0] + r[2], r[1] + r[3]]);
            Some([
                x0.min(r[0]),
                y0.min(r[1]),
                x1.max(r[0] + r[2]),
                y1.max(r[1] + r[3]),
            ])
        });
        if let Some([x0, y0, x1, y1]) = b {
            self.camera
                .fit([x0, y0, x1 - x0, y1 - y0], self.view[2], self.view[3]);
        }
    }
}
