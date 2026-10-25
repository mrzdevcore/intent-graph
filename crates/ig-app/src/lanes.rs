//! Blueprint lanes and system icon categories.

use ig_core::config::Config;
use ig_core::model::Node;
use ig_core::rules::Layers;
use ig_core::workspace::globset;

/// Blueprint lanes: a node goes to the first lane whose globs or layers match; systems go in a
/// last "Systems" lane; test files go nowhere.
pub struct LaneMap<'c> {
    config: &'c Config,
    layers: &'c Layers<'c>,
    globs: Vec<(globset::GlobSet, globset::GlobSet)>,
    tests: globset::GlobSet,
}

impl<'c> LaneMap<'c> {
    pub fn new(config: &'c Config, layers: &'c Layers<'c>) -> LaneMap<'c> {
        let set = |g: &[String]| globset(g).unwrap_or_else(|_| globset::GlobSet::empty());
        LaneMap {
            config,
            layers,
            globs: config
                .blueprint
                .lanes
                .iter()
                .map(|l| (set(&l.matches), set(&l.exclude)))
                .collect(),
            tests: set(&config.tests),
        }
    }

    pub fn of(&self, n: &Node) -> Option<usize> {
        let lanes = &self.config.blueprint.lanes;
        if n.kind == "system" {
            return Some(lanes.len());
        }
        if self.tests.is_match(&n.file) {
            return None;
        }
        let layer = self.layers.of(&n.file);
        lanes.iter().enumerate().position(|(i, l)| {
            let (inc, exc) = &self.globs[i];
            !exc.is_match(&n.file)
                && (inc.is_match(&n.file) || layer.is_some_and(|x| l.layers.iter().any(|y| y == x)))
        })
    }

    pub fn name(&self, i: Option<usize>) -> String {
        let lanes = &self.config.blueprint.lanes;
        match i {
            Some(i) if i < lanes.len() => lanes[i].name.clone(),
            Some(_) => "Systems".into(),
            None => "Other".into(),
        }
    }
}

#[rustfmt::skip]
const SYSTEM_CATEGORIES: &[(&str, &[&str])] = &[
    ("database", &["postgres", "pg", "mysql", "mariadb", "mongo", "mongoose", "prisma", "typeorm", "knex", "sequelize", "sqlite", "redis", "ioredis", "dynamo", "firestore", "supabase", "cassandra"]),
    // Never plain "elastic": "Elastic APM" is monitoring.
    ("search", &["elastic search", "elasticsearch", "@elastic/elasticsearch", "algolia", "meili", "typesense", "opensearch"]),
    ("monitoring", &["apm", "datadog", "sentry", "newrelic", "opentelemetry", "grafana", "prometheus", "logrocket"]),
    ("auth", &["auth0", "okta", "keycloak", "cognito", "clerk", "firebase-auth", "next-auth", "passport"]),
    ("cms", &["contentful", "strapi", "sanity", "prismic", "storyblok", "directus", "ghost"]),
    ("analytics", &["power bi", "powerbi", "segment", "mixpanel", "amplitude", "google-analytics", "gtag", "matomo", "posthog"]),
    ("queue", &["kafka", "rabbit", "amqp", "sqs", "bull", "nats"]),
    ("storage", &["s3", "@aws-sdk/client-s3", "minio", "cloudinary", "blob"]),
    ("payment", &["stripe", "paypal", "adyen"]),
    ("email", &["sendgrid", "mailgun", "nodemailer", "postmark", "ses"]),
];

/// The icon category of a blueprint system: the first whose keywords occur (case-insensitive) in
/// its name or one of its packages, else "service".
pub fn system_category(name: &str, packages: &[String]) -> &'static str {
    let hay: Vec<String> = std::iter::once(name)
        .chain(packages.iter().map(String::as_str))
        .map(str::to_lowercase)
        .collect();
    SYSTEM_CATEGORIES
        .iter()
        .find(|(_, kws)| kws.iter().any(|k| hay.iter().any(|h| h.contains(k))))
        .map_or("service", |(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_categories() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(system_category("PostgreSQL", &p(&["pg"])), "database");
        assert_eq!(system_category("MongoDB", &p(&["mongoose"])), "database");
        assert_eq!(
            system_category("Elastic APM", &p(&["elastic-apm-node"])),
            "monitoring"
        );
        assert_eq!(
            system_category("Search", &p(&["@elastic/elasticsearch"])),
            "search"
        );
        assert_eq!(
            system_category("Auth0", &p(&["@auth0/auth0-react"])),
            "auth"
        );
        assert_eq!(system_category("Contentful", &p(&["contentful"])), "cms");
        assert_eq!(
            system_category("Power BI", &p(&["powerbi-client"])),
            "analytics"
        );
        assert_eq!(system_category("Maite BFF", &p(&["@maite/sdk"])), "service");
    }
}
