/** True in the desktop app's build (`build:app`), false in `ig view`'s: the boards are app-only. */
declare const __BOARDS__: boolean;

declare module "cytoscape-fcose" {
  const fcose: cytoscape.Ext;
  export default fcose;
}
