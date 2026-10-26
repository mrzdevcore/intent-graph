/** True in the desktop app's build (`build:app`), false in `ig view`'s: the boards are app-only. */
declare const __BOARDS__: boolean;

// d3-force ships no types: what layout.ts uses of it.
declare module "d3-force" {
  interface Force<N> {
    (alpha: number): void;
    initialize?(nodes: N[], random: () => number): void;
  }
  interface Simulation<N> {
    force(name: string, f: Force<N>): this;
    randomSource(r: () => number): this;
    alphaDecay(d: number): this;
    tick(n?: number): this;
    stop(): this;
  }
  interface Link<N, L> extends Force<N> {
    distance(d: number | ((l: L) => number)): this;
    strength(s: number | ((l: L) => number)): this;
  }
  interface ByNode<N> extends Force<N> {
    strength(s: number | ((n: N) => number)): this;
  }
  interface ManyBody<N> extends ByNode<N> {
    theta(t: number): this;
  }
  interface Collide<N> extends Force<N> {
    iterations(n: number): this;
  }
  export function forceSimulation<N>(nodes: N[]): Simulation<N>;
  export function forceLink<N, L>(links: L[]): Link<N, L>;
  export function forceManyBody<N>(): ManyBody<N>;
  export function forceCollide<N>(radius: (n: N) => number): Collide<N>;
  export function forceX<N>(x: number): ByNode<N>;
  export function forceY<N>(y: number): ByNode<N>;
}
