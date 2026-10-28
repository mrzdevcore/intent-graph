//! Outside systems known by the npm packages that talk to them.

use super::facts::BoxKind;

pub struct Known {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: BoxKind,
    /// A simple-icons slug.
    pub logo: Option<&'static str>,
    /// Package names; a trailing `*` matches a prefix.
    pub packages: &'static [&'static str],
}

const fn k(
    id: &'static str,
    title: &'static str,
    kind: BoxKind,
    logo: Option<&'static str>,
    packages: &'static [&'static str],
) -> Known {
    Known {
        id,
        title,
        kind,
        logo,
        packages,
    }
}

use BoxKind::{External, Queue, Store};

#[rustfmt::skip]
pub const KNOWN: &[Known] = &[
    k("postgres", "PostgreSQL", Store, Some("postgresql"), &["pg", "postgres", "pg-promise", "@neondatabase/serverless", "@vercel/postgres"]),
    k("mysql", "MySQL", Store, Some("mysql"), &["mysql", "mysql2"]),
    k("mongodb", "MongoDB", Store, Some("mongodb"), &["mongodb", "mongoose"]),
    k("redis", "Redis", Store, Some("redis"), &["redis", "ioredis", "@upstash/redis"]),
    k("sqlite", "SQLite", Store, Some("sqlite"), &["better-sqlite3", "sqlite3"]),
    k("elasticsearch", "Elasticsearch", Store, Some("elasticsearch"), &["@elastic/elasticsearch"]),
    k("prisma", "Database (Prisma)", Store, Some("prisma"), &["@prisma/client"]),
    k("drizzle", "Database (Drizzle)", Store, None, &["drizzle-orm"]),
    k("bull", "Bull queue (Redis)", Queue, Some("redis"), &["bullmq", "bull"]),
    k("kafka", "Kafka", Queue, Some("apachekafka"), &["kafkajs"]),
    k("rabbitmq", "RabbitMQ", Queue, Some("rabbitmq"), &["amqplib"]),
    k("sqs", "AWS SQS", Queue, Some("amazonsqs"), &["@aws-sdk/client-sqs"]),
    k("s3", "AWS S3", Store, Some("amazons3"), &["@aws-sdk/client-s3"]),
    k("stripe", "Stripe", External, Some("stripe"), &["stripe", "@stripe/stripe-js"]),
    k("auth0", "Auth0", External, Some("auth0"), &["auth0", "@auth0/*"]),
    k("sentry", "Sentry", External, Some("sentry"), &["@sentry/*"]),
    k("elastic-apm", "Elastic APM", External, None, &["elastic-apm-node", "@elastic/apm-rum*"]),
    k("firebase", "Firebase", External, Some("firebase"), &["firebase", "firebase-admin"]),
    k("supabase", "Supabase", External, Some("supabase"), &["@supabase/*"]),
    k("openai", "OpenAI", External, Some("openai"), &["openai"]),
    k("anthropic", "Anthropic", External, Some("anthropic"), &["@anthropic-ai/sdk"]),
    k("sendgrid", "SendGrid", External, None, &["@sendgrid/*"]),
    k("twilio", "Twilio", External, Some("twilio"), &["twilio"]),
];

fn by_id(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// The known system titled `title` (a detected system's graph node name).
pub fn by_title(title: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.title == title)
}

/// The system an import of `pkg` stands for. Prisma is the database its schema's datasource
/// names (`schema` reads it, only when needed).
pub fn system(pkg: &str, schema: impl FnOnce() -> Option<String>) -> Option<&'static Known> {
    let k = KNOWN.iter().find(|k| {
        k.packages.iter().any(|p| {
            p.strip_suffix('*')
                .map_or(*p == pkg, |pre| pkg.starts_with(pre))
        })
    })?;
    if k.id != "prisma" {
        return Some(k);
    }
    let real = schema().and_then(|s| match provider(&s)? {
        "postgresql" => by_id("postgres"),
        "mysql" => by_id("mysql"),
        "sqlite" => by_id("sqlite"),
        "mongodb" => by_id("mongodb"),
        _ => None,
    });
    Some(real.unwrap_or(k))
}

/// The `provider` of a Prisma schema's `datasource` block.
fn provider(schema: &str) -> Option<&str> {
    let block = schema.split("datasource").nth(1)?.split('}').next()?;
    block.lines().find_map(|l| {
        let (key, v) = l.split_once('=')?;
        (key.trim() == "provider").then(|| v.trim().trim_matches('"'))
    })
}

#[cfg(test)]
mod tests;
