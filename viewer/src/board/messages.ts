// The board's failure messages: also used by App.svelte when the board chunk itself fails to load.
export const loadFailed = (reason: string) =>
  `The board engine failed to load: ${reason}. Rebuild the app with \`make app\`.`;
export const noWebgl2 = (reason: string) =>
  `This board needs WebGL2, which this system doesn't offer (${reason}). Graph, Code and chat still work.`;
