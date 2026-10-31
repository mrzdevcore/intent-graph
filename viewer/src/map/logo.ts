import { brand, iconFor } from "../icons";
import type { MapBox } from "./types";

export type Logo = { path: string; color: string; slug: string };

/** A brand by simple-icons slug (reuses the viewer's bundled brand table; the rest of simple-icons is not bundled). */
export function logo(slug: string): { path: string; color: string } | null {
  const i = iconFor(slug, [], "");
  return i.brand ? { path: i.path, color: i.color } : null;
}
export const logoPath = (slug: string): string | null => logo(slug)?.path ?? null;

// relative luminance of #rrggbb, 0 (black) to 1 (white)
const lum = (hex: string) => {
  const c = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.04 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
};

/** A box's logo: its `logo` field, else the first brand in its tech (the whole string, then each word); a brand too
 *  dark to see on a dark map (Next, Ollama, Kafka…) draws in the map's foreground, on a CLI's terminal in the
 *  terminal's (dark in both themes). */
export function boxLogo(b: MapBox, shape?: string): Logo | null {
  const tech = b.tech?.value.toLowerCase() ?? "";
  const keys = b.logo ? [b.logo.value.toLowerCase(), b.logo.value.toLowerCase().split(" ")[0]] : [tech, ...tech.split(/[\s·,/+()]+/)];
  const si = keys.map((k) => brand(k)).find(Boolean);
  return si ? { path: si.path, color: lum(si.hex) < 0.05 ? (shape === "cli" ? "var(--kind-cli-fg)" : "var(--map-fg)") : "#" + si.hex, slug: si.slug } : null;
}
