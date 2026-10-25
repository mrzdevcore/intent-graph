//! Language-neutral HTTP route matching: URL segments, route constants, call-to-route matching.

use std::collections::HashMap;

/// URL path segments with dynamic parts (`${x}`, `{param}`, `:param`) as `:`, without scheme, host,
/// query or fragment.
pub fn url_segments(url: &str) -> Vec<String> {
    let mut u = url;
    if let Some(i) = u.find("://") {
        u = &u[i + 3..];
        u = u.find('/').map(|j| &u[j..]).unwrap_or("");
    }
    let u = u.split(['?', '#']).next().unwrap_or("");
    u.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.contains('\u{0}') {
                "\u{0}".to_string()
            } else if s.contains(':') || s.contains('{') || s.contains('*') {
                ":".to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

pub fn seg_match(route: &[String], call: &[String], loose: bool) -> bool {
    route.len() == call.len()
        && route
            .iter()
            .zip(call)
            .all(|(r, c)| r == c || (loose && r == ":"))
}

/// `${NAME}` in a route path replaced by a same-file string constant.
pub fn substitute(path: &str, consts: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('}') else { break };
        let name = rest[i + 2..i + j].trim();
        // Unknown: a segment that never matches, so the route can't be linked by accident.
        out.push_str(consts.get(name).map(String::as_str).unwrap_or("\u{0}"));
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out
}

/// A route node name, "METHOD /path" (or just "/path" for any method, `*`): its method and its
/// path with same-file constants (`consts`) substituted.
pub fn route(name: &str, consts: &HashMap<String, String>) -> (String, String) {
    let (m, p) = match name.split_once(' ') {
        Some((m, p)) if m.chars().all(|c| c.is_ascii_alphabetic()) => (m.to_ascii_uppercase(), p),
        _ => ("*".to_string(), name),
    };
    let p = p.trim();
    // A bare identifier path (`path: BASE_PATH`) is a same-file constant too.
    let p = match consts.get(p) {
        Some(c)
            if p.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$') =>
        {
            c.clone()
        }
        _ => substitute(p.trim_matches(['\'', '"', '`']), consts),
    };
    (m, p)
}

/// The route ids (`routes`: method, path segments, id) a client call matches, and the edge
/// confidence: strict segment match ("resolved"), then loose, then a suffix of the call ("guessed").
/// Leading dynamic segments and `prefix` are stripped from the call first.
pub fn match_call<'r>(
    routes: &[(String, Vec<String>, &'r str)],
    method: &str,
    url: &str,
    prefix: &str,
) -> (Vec<&'r str>, &'static str) {
    let mut call = url_segments(url);
    while call.first().is_some_and(|s| s == ":") {
        call.remove(0);
    }
    let prefix = url_segments(prefix);
    if !prefix.is_empty() && call.starts_with(&prefix) {
        call.drain(..prefix.len());
    }
    let method_ok = |m: &str| m == "*" || m == method;
    let pick = |loose: bool| -> Vec<&str> {
        routes
            .iter()
            .filter(|(m, r, _)| method_ok(m) && seg_match(r, &call, loose))
            .map(|(_, _, id)| *id)
            .collect()
    };
    // Last resort: the call has extra leading segments (an `/api` prefix added by a proxy).
    let suffix = || -> Vec<&str> {
        routes
            .iter()
            .filter(|(m, r, _)| {
                method_ok(m)
                    && r.len() >= 2
                    && r.len() < call.len()
                    && seg_match(r, &call[call.len() - r.len()..], true)
            })
            .map(|(_, _, id)| *id)
            .collect()
    };
    match pick(false) {
        v if !v.is_empty() => (v, "resolved"),
        _ => match pick(true) {
            v if !v.is_empty() => (v, "guessed"),
            _ => (suffix(), "guessed"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_normalisation() {
        assert_eq!(
            url_segments("/api/lines/:/notes?x=1"),
            ["api", "lines", ":", "notes"]
        );
        assert_eq!(url_segments("https://h:8080/api/{id}/x"), ["api", ":", "x"]);
        assert_eq!(url_segments("/api/item-:/x"), ["api", ":", "x"]);
        // Strict: segments equal. Loose: a route parameter also matches a literal in the call,
        // but a dynamic call segment never matches a literal route segment.
        assert!(!seg_match(
            &url_segments("/api/:/:"),
            &url_segments("/api/1/:"),
            false
        ));
        assert!(seg_match(
            &url_segments("/api/:/:"),
            &url_segments("/api/1/:"),
            true
        ));
        assert!(!seg_match(
            &url_segments("/api/:/available"),
            &url_segments("/api/:/:"),
            true
        ));
    }

    #[test]
    fn routes_and_calls() {
        let consts: HashMap<String, String> = [("BASE".to_string(), "/api/v1".to_string())].into();
        assert_eq!(
            route("get ${BASE}/x", &consts),
            ("GET".into(), "/api/v1/x".into())
        );
        assert_eq!(route("BASE", &consts), ("*".into(), "/api/v1".into()));
        assert_eq!(route("'/${NOPE}/y'", &consts).1, "/\u{0}/y");
        let routes = vec![
            ("GET".to_string(), url_segments("/items/:id"), "r1"),
            ("*".to_string(), url_segments("/items/new"), "r2"),
        ];
        assert_eq!(
            match_call(&routes, "GET", "/items/new", ""),
            (vec!["r2"], "resolved")
        );
        assert_eq!(
            match_call(&routes, "GET", "/items/7", ""),
            (vec!["r1"], "guessed")
        );
        assert_eq!(
            match_call(&routes, "GET", "/app/api/items/7", "/app"),
            (vec!["r1"], "guessed")
        );
        assert_eq!(
            match_call(&routes, "POST", "/items/7", "").0,
            Vec::<&str>::new()
        );
    }
}
