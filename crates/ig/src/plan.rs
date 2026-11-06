use ig_app::map::origin::Plan;
use serde_json::{Value, json};

pub fn json(p: &Plan) -> Value {
    json!({"build": p.build, "move": p.moves, "drift": p.drift, "notRead": p.not_read})
}
