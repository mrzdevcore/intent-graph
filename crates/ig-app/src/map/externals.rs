//! Outside systems known by the client libraries that talk to them: npm packages, Python's
//! top-level modules, Rust crates and Go modules alike (ponytail: one list for all; a name two ecosystems share means one system).

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
    k("postgres", "PostgreSQL", Store, Some("postgresql"), &["pg", "postgres", "pg-promise", "@neondatabase/serverless", "@vercel/postgres", "psycopg2", "psycopg", "asyncpg", "tokio_postgres", "github.com/jackc/pgx*", "github.com/lib/pq"]),
    k("mysql", "MySQL", Store, Some("mysql"), &["mysql", "mysql2", "MySQLdb", "pymysql", "aiomysql", "mysql_async", "github.com/go-sql-driver/mysql"]),
    k("mongodb", "MongoDB", Store, Some("mongodb"), &["mongodb", "mongoose", "pymongo", "motor", "go.mongodb.org/mongo-driver*"]),
    k("redis", "Redis", Store, Some("redis"), &["redis", "ioredis", "@upstash/redis", "aioredis", "github.com/redis/go-redis*", "github.com/go-redis/redis*"]),
    k("sqlite", "SQLite", Store, Some("sqlite"), &["better-sqlite3", "sqlite3", "aiosqlite", "rusqlite", "github.com/mattn/go-sqlite3", "modernc.org/sqlite"]),
    k("elasticsearch", "Elasticsearch", Store, Some("elasticsearch"), &["@elastic/elasticsearch", "elasticsearch", "github.com/elastic/go-elasticsearch*"]),
    k("prisma", "Database (Prisma)", Store, Some("prisma"), &["@prisma/client"]),
    k("drizzle", "Database (Drizzle)", Store, None, &["drizzle-orm"]),
    k("database", "Database", Store, None, &[]),
    k("bull", "Bull queue (Redis)", Queue, Some("redis"), &["bullmq", "bull"]),
    k("kafka", "Kafka", Queue, Some("apachekafka"), &["kafkajs", "kafka", "confluent_kafka", "aiokafka", "rdkafka", "github.com/segmentio/kafka-go", "github.com/IBM/sarama", "github.com/confluentinc/confluent-kafka-go*"]),
    k("rabbitmq", "RabbitMQ", Queue, Some("rabbitmq"), &["amqplib", "pika", "aio_pika", "lapin", "github.com/rabbitmq/amqp091-go"]),
    k("sqs", "AWS SQS", Queue, Some("amazonsqs"), &["@aws-sdk/client-sqs", "aws_sdk_sqs", "github.com/aws/aws-sdk-go-v2/service/sqs"]),
    k("s3", "AWS S3", Store, Some("amazons3"), &["@aws-sdk/client-s3", "aws_sdk_s3", "github.com/aws/aws-sdk-go-v2/service/s3"]),
    k("stripe", "Stripe", External, Some("stripe"), &["stripe", "@stripe/stripe-js", "github.com/stripe/stripe-go*"]),
    k("auth0", "Auth0", External, Some("auth0"), &["auth0", "@auth0/*"]),
    k("sentry", "Sentry", External, Some("sentry"), &["@sentry/*", "sentry_sdk", "sentry", "github.com/getsentry/sentry-go"]),
    k("elastic-apm", "Elastic APM", External, None, &["elastic-apm-node", "@elastic/apm-rum*", "elasticapm"]),
    k("firebase", "Firebase", External, Some("firebase"), &["firebase", "firebase-admin", "firebase_admin"]),
    k("supabase", "Supabase", External, Some("supabase"), &["@supabase/*", "supabase"]),
    k("openai", "OpenAI", External, Some("openai"), &["openai", "async_openai", "github.com/openai/openai-go*", "github.com/sashabaranov/go-openai"]),
    k("anthropic", "Anthropic", External, Some("anthropic"), &["@anthropic-ai/sdk", "anthropic", "github.com/anthropics/anthropic-sdk-go"]),
    k("sendgrid", "SendGrid", External, None, &["@sendgrid/*", "sendgrid"]),
    k("twilio", "Twilio", External, Some("twilio"), &["twilio", "github.com/twilio/twilio-go"]),
];

fn by_id(id: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|k| k.id == id)
}

/// The databases a schema's tables may be kept in (`for_dialect`'s, and the ORMs').
pub fn relational(id: &str) -> bool {
    [
        "postgres", "mysql", "sqlite", "mongodb", "prisma", "drizzle", "supabase", "database",
    ]
    .contains(&id)
}

/// The database a schema of `dialect` is kept in; "Database" when it does not say.
pub fn for_dialect(dialect: Option<&str>) -> &'static Known {
    let id = match dialect {
        Some("postgresql") => "postgres",
        Some("mysql") => "mysql",
        Some("sqlite") => "sqlite",
        Some("mongodb") => "mongodb",
        _ => "database",
    };
    by_id(id).expect("a known database")
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
