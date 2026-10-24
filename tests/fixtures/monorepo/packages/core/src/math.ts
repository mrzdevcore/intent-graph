/** Adds numbers. @intent CORE-1 */
export function add(a: number, b: number): number {
  return a + b;
}

export const double = (x: number) => add(x, x);

function overloaded(a: string): string;
function overloaded(a: number): number;
function overloaded(a: any) {
  return a ?? 0;
}
