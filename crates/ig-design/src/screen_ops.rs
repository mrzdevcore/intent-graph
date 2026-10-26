//! The screen ops of `apply`: same all-or-nothing path as the architecture ops.

use crate::screens::{MAX_SCREENS, PROP_NAMES};
use crate::{
    Bridge, Design, DesignError, Nav, Op, Piece, PieceKind, Props, Screen, Touched, VERSION,
    new_piece_id,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Fields `screen_update` may change; never the id or the kind. An empty string clears `from`
/// and `to`; a null prop is removed.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct PiecePatch {
    pub label: Option<String>,
    pub w: Option<i32>,
    pub h: Option<i32>,
    pub props: Option<Map<String, Value>>,
    pub points: Option<Vec<[i32; 3]>>,
    pub from: Option<String>,
    pub to: Option<String>,
}

type Screens = BTreeMap<String, Screen>;

fn push(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.into());
    }
}

fn bad<T>(msg: String) -> Result<T, DesignError> {
    Err(DesignError::Invalid(msg))
}

fn screen<'a>(sc: &'a mut Screens, id: &str) -> Result<&'a mut Screen, DesignError> {
    sc.get_mut(id)
        .ok_or_else(|| DesignError::Invalid(format!("no screen {id}")))
}

fn piece<'a>(s: &'a mut Screen, id: &str) -> Result<&'a mut Piece, DesignError> {
    let sid = s.id.clone();
    s.elements
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| DesignError::Invalid(format!("{sid}: no element {id}")))
}

/// `base` with `patch` laid over it; a null removes a prop, a name no kind has is refused.
fn overlay(id: &str, base: &Props, patch: &Map<String, Value>) -> Result<Props, DesignError> {
    let Value::Object(mut m) = serde_json::to_value(base).expect("props serialize") else {
        unreachable!()
    };
    for (k, v) in patch {
        if !PROP_NAMES.contains(&k.as_str()) {
            return bad(format!("{id}: unknown prop {k}"));
        }
        if v.is_null() {
            m.remove(k);
        } else {
            m.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(Value::Object(m)).map_err(|e| DesignError::Invalid(format!("{id}: {e}")))
}

fn clear(slot: &mut Option<String>, v: &Option<String>) {
    if let Some(v) = v {
        *slot = (!v.is_empty()).then(|| v.clone());
    }
}

fn size(k: PieceKind) -> (i32, i32) {
    use PieceKind::*;
    match k {
        Button => (120, 40),
        Input | Select => (240, 40),
        Checkbox => (160, 24),
        Table | List | Card | Frame => (400, 240),
        Tabs => (400, 40),
        Modal => (480, 320),
        Text => (240, 24),
        Heading => (240, 32),
        Image => (240, 160),
        Icon => (24, 24),
        Pen | Arrow => (100, 100),
        Note => (200, 120),
    }
}

fn fresh_screen_id(sc: &Screens, title: &str) -> String {
    let slug = title
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base: String = format!("screen-{slug}")
        .chars()
        .take(50)
        .collect::<String>()
        .trim_end_matches('-')
        .into();
    if !sc.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !sc.contains_key(id))
        .unwrap()
}

/// Runs a screen op; false when `op` is not one.
pub(crate) fn step(
    d: &mut Design,
    sc: &mut Screens,
    op: &Op,
    t: &mut Touched,
) -> Result<bool, DesignError> {
    match op {
        Op::AddScreen {
            id,
            title,
            variant_of,
            state,
        } => {
            if title.trim().is_empty() {
                return bad("empty title".into());
            }
            let id = id.clone().unwrap_or_else(|| fresh_screen_id(sc, title));
            if sc.contains_key(&id) {
                return bad(format!("screen {id} exists"));
            }
            let x = sc.values().map(|s| s.x + s.w + 80).max().unwrap_or(0);
            if sc.len() >= MAX_SCREENS {
                return bad(format!("at most {MAX_SCREENS} screens per feature"));
            }
            let s = Screen {
                version: VERSION,
                id: id.clone(),
                title: title.clone(),
                variant_of: variant_of.clone(),
                state: state.clone(),
                x,
                y: 0,
                w: 1280,
                h: 800,
                background: None,
                elements: vec![],
                navs: vec![],
            };
            push(&mut t.screens, &id);
            sc.insert(id, s);
        }
        Op::RemoveScreen { id } => {
            if !sc.contains_key(id) {
                return bad(format!("no screen {id}"));
            }
            if let Some(v) = sc.values().find(|s| s.variant_of.as_deref() == Some(id)) {
                return bad(format!(
                    "screen {id} is the base of {}: remove that first",
                    v.id
                ));
            }
            sc.remove(id);
            for s in sc.values_mut() {
                let n = s.navs.len();
                s.navs.retain(|v| v.to != *id);
                if s.navs.len() != n {
                    push(&mut t.screens, &s.id);
                }
            }
            let pre = format!("{id}/");
            drop_bridges(d, t, |b| b.from.starts_with(&pre));
            push(&mut t.screens, id); // the removed id is touched too
        }
        Op::ScreenAdd {
            screen: sid,
            id,
            kind,
            label,
            props,
            x,
            y,
        } => {
            let s = screen(sc, sid)?;
            let id = id.clone().unwrap_or_else(|| new_piece_id(s, *kind, label));
            let props = overlay(
                &id,
                &Props::default(),
                props.as_ref().unwrap_or(&Map::new()),
            )?;
            let (w, h) = size(*kind);
            s.elements.push(Piece {
                id: id.clone(),
                kind: *kind,
                label: label.clone(),
                props,
                x: x.unwrap_or(0),
                y: y.unwrap_or(0),
                w,
                h,
                points: None,
                from: None,
                to: None,
            });
            push(&mut t.screens, sid);
        }
        Op::ScreenUpdate {
            screen: sid,
            id,
            set: p,
        } => {
            let p_ = piece(screen(sc, sid)?, id)?;
            if let Some(l) = &p.label {
                p_.label = l.clone();
            }
            (p_.w, p_.h) = (p.w.unwrap_or(p_.w), p.h.unwrap_or(p_.h));
            if let Some(m) = &p.props {
                p_.props = overlay(id, &p_.props, m)?;
            }
            if let Some(pts) = &p.points {
                p_.points = Some(pts.clone());
            }
            clear(&mut p_.from, &p.from);
            clear(&mut p_.to, &p.to);
            push(&mut t.screens, sid);
        }
        Op::ScreenMove {
            screen: sid,
            id,
            x,
            y,
        } => {
            let p = piece(screen(sc, sid)?, id)?;
            (p.x, p.y) = (*x, *y);
            push(&mut t.screens, sid);
        }
        Op::ScreenRemove { screen: sid, id } => {
            let s = screen(sc, sid)?;
            piece(s, id)?;
            s.elements.retain(|p| p.id != *id);
            s.navs.retain(|n| n.from != *id);
            for p in &mut s.elements {
                if p.from.as_deref() == Some(id) {
                    p.from = None;
                }
                if p.to.as_deref() == Some(id) {
                    p.to = None;
                }
            }
            let key = format!("{sid}/{id}");
            drop_bridges(d, t, |b| b.from == key);
            push(&mut t.screens, sid);
        }
        Op::Nav {
            screen: sid,
            from,
            to,
            label,
        } => {
            let s = screen(sc, sid)?;
            let n = Nav {
                from: from.clone(),
                to: to.clone(),
                label: label.clone(),
            };
            match s.navs.iter_mut().find(|v| v.from == *from && v.to == *to) {
                Some(v) => *v = n,
                None => s.navs.push(n),
            }
            push(&mut t.screens, sid);
        }
        Op::Unnav {
            screen: sid,
            from,
            to,
        } => {
            let s = screen(sc, sid)?;
            let n = s.navs.len();
            s.navs.retain(|v| !(v.from == *from && v.to == *to));
            if s.navs.len() == n {
                return bad(format!("{sid}: no nav {from} to {to}"));
            }
            push(&mut t.screens, sid);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn drop_bridges(d: &mut Design, t: &mut Touched, hit: impl Fn(&Bridge) -> bool) {
    d.bridges.retain(|b| {
        let h = hit(b);
        if h {
            push(&mut t.bridges, &b.key());
        }
        !h
    });
}
