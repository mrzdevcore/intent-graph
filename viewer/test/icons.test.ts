import { expect, test } from "vitest";
import { Cloud, Database } from "lucide-static";
import { siElastic, siPostgresql } from "simple-icons";
import { iconFor, iconUri } from "../src/icons";

test("brand icons by name or package, lucide by category otherwise", () => {
  expect(iconFor("PostgreSQL", ["pg"], "database")).toEqual({ brand: true, path: siPostgresql.path, color: "#" + siPostgresql.hex });
  expect(iconFor("Elastic APM", ["elastic-apm-node"], "monitoring")).toEqual({ brand: true, path: siElastic.path, color: "#" + siElastic.hex });
  expect(iconFor("Acme BFF", ["@acme/sdk"], "service")).toEqual({ brand: false, svg: Cloud });
  expect(iconFor("Ledger", ["@acme/ledger-db"], "database")).toEqual({ brand: false, svg: Database });
});

test("brand logos sit on their own light tile, lucide icons do not", () => {
  const svg = (name: string, cat: string) => decodeURIComponent(iconUri(iconFor(name, [], cat), "#eee"));
  expect(svg("OpenTelemetry", "monitoring")).toContain('<rect width="24" height="24" rx="5" fill="#fff"');
  expect(svg("Acme BFF", "service")).not.toContain("<rect");
});
