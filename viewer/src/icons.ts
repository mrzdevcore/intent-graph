// System icons: a brand logo (simple-icons) for the systems below, else a lucide icon by category.
// Only what is imported here is bundled. simple-icons 16 no longer ships Segment, Amplitude,
// Amazon S3, SendGrid or Power BI; those get their category's icon.

import { Activity, Archive, ChartBar, Cloud, CreditCard, Database, FileText, KeyRound, ListOrdered, Mail, Search } from "lucide-static";
import {
  siAlgolia, siApachekafka, siAuth0, siClerk, siCloudinary, siContentful, siDatadog, siDirectus, siElastic, siElasticsearch,
  siFirebase, siGhost, siGoogleanalytics, siGrafana, siKeycloak, siMailgun, siMariadb, siMatomo, siMeilisearch, siMinio,
  siMixpanel, siMongodb, siMysql, siNatsdotio, siNewrelic, siOkta, siOpensearch, siOpentelemetry, siPaypal, siPosthog,
  siPostgresql, siPrismic, siPrometheus, siRabbitmq, siRedis, siSanity, siSentry, siSqlite, siStoryblok, siStrapi, siStripe,
  siSupabase, type SimpleIcon,
} from "simple-icons";

/** Lower-cased system names, packages and package scopes → brand. */
const BRANDS: Record<string, SimpleIcon> = {
  postgres: siPostgresql, postgresql: siPostgresql, pg: siPostgresql,
  mysql: siMysql, mysql2: siMysql, mariadb: siMariadb,
  mongo: siMongodb, mongodb: siMongodb, mongoose: siMongodb,
  redis: siRedis, ioredis: siRedis,
  sqlite: siSqlite, sqlite3: siSqlite, "better-sqlite3": siSqlite,
  supabase: siSupabase, firebase: siFirebase, "firebase-admin": siFirebase,
  elasticsearch: siElasticsearch, "elastic search": siElasticsearch, "@elastic/elasticsearch": siElasticsearch,
  elastic: siElastic, "elastic apm": siElastic, "elastic-apm-node": siElastic,
  algolia: siAlgolia, algoliasearch: siAlgolia, meilisearch: siMeilisearch, opensearch: siOpensearch,
  datadog: siDatadog, "dd-trace": siDatadog, sentry: siSentry, newrelic: siNewrelic, "new relic": siNewrelic,
  opentelemetry: siOpentelemetry, grafana: siGrafana, prometheus: siPrometheus, "prom-client": siPrometheus,
  auth0: siAuth0, "auth0-js": siAuth0, okta: siOkta, keycloak: siKeycloak, "keycloak-js": siKeycloak, clerk: siClerk,
  contentful: siContentful, strapi: siStrapi, sanity: siSanity, prismic: siPrismic, prismicio: siPrismic,
  storyblok: siStoryblok, directus: siDirectus, ghost: siGhost, tryghost: siGhost,
  mixpanel: siMixpanel, "mixpanel-browser": siMixpanel, "google analytics": siGoogleanalytics, googleanalytics: siGoogleanalytics,
  "google-analytics": siGoogleanalytics, matomo: siMatomo, posthog: siPosthog, "posthog-js": siPosthog,
  kafka: siApachekafka, "apache kafka": siApachekafka, kafkajs: siApachekafka, rabbitmq: siRabbitmq, amqplib: siRabbitmq,
  nats: siNatsdotio, minio: siMinio, cloudinary: siCloudinary, stripe: siStripe, paypal: siPaypal,
  mailgun: siMailgun, "mailgun.js": siMailgun,
};

const BY_CATEGORY: Record<string, string> = {
  database: Database, search: Search, monitoring: Activity, auth: KeyRound, cms: FileText, analytics: ChartBar,
  queue: ListOrdered, storage: Archive, payment: CreditCard, email: Mail, service: Cloud,
};

export type Icon = { brand: true; path: string; color: string } | { brand: false; svg: string };

/** The brand of the name, a package, a package's scope or the name's first word; else the category's icon. */
export function iconFor(name: string, packages: string[], category: string): Icon {
  const pkgs = packages.map((p) => p.toLowerCase());
  const keys = [name.toLowerCase(), ...pkgs, ...pkgs.map((p) => p.replace(/^@([^/]+)\/.*$/, "$1")), name.toLowerCase().split(" ")[0]];
  const hit = keys.map((k) => BRANDS[k]).find(Boolean);
  if (hit) return { brand: true, path: hit.path, color: "#" + hit.hex };
  return { brand: false, svg: BY_CATEGORY[category] || Cloud };
}

/** A data URI for the icon: brands in their colour, lucide icons stroked in `fg`. */
export function iconUri(icon: Icon, fg: string): string {
  const svg = icon.brand
    ? `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><path fill="${icon.color}" d="${icon.path}"/></svg>`
    : icon.svg.replace(/currentColor/g, fg);
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg);
}
