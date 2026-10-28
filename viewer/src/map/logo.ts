import { iconFor } from "../icons";

/** A brand by simple-icons slug (reuses the viewer's bundled brand table; the rest of simple-icons is not bundled). */
export function logo(slug: string): { path: string; color: string } | null {
  const i = iconFor(slug, [], "");
  return i.brand ? { path: i.path, color: i.color } : null;
}
export const logoPath = (slug: string): string | null => logo(slug)?.path ?? null;
