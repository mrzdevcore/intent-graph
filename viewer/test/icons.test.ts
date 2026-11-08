import { expect, test } from "vitest";
import { Cloud, Database } from "lucide-static";
import { siElastic, siPostgresql } from "simple-icons";
import { iconFor } from "../src/icons";

test("brand icons by name or package, lucide by category otherwise", () => {
  expect(iconFor("PostgreSQL", ["pg"], "database")).toEqual({ brand: true, path: siPostgresql.path, color: "#" + siPostgresql.hex });
  expect(iconFor("Elastic APM", ["elastic-apm-node"], "monitoring")).toEqual({ brand: true, path: siElastic.path, color: "#" + siElastic.hex });
  expect(iconFor("Shop BFF", ["@shop/sdk"], "service")).toEqual({ brand: false, svg: Cloud });
  expect(iconFor("Ledger", ["@acme/ledger-db"], "database")).toEqual({ brand: false, svg: Database });
});
