<!-- The checks bar under the board: problem counts, the size of the change, how long the check
     took, the findings no element carries (the rest show on the board), and the save status. -->
<script lang="ts">
  import type { DesignCheck } from "../api";

  let { check, error, status, failed }: {
    check: DesignCheck | null;
    /** The check itself failed. */
    error: string | null;
    status: string;
    /** The status is a save or read error. */
    failed: boolean;
  } = $props();

  const count = (s: string) => check?.findings.filter((f) => f.severity === s).length ?? 0;
  const loose = $derived(check?.findings.filter((f) => !f.target) ?? []);
  const volume = $derived.by(() => {
    const v = check?.volume;
    if (!v) return "";
    const nodes = Object.values(v.newNodes ?? {}).reduce((a, n) => a + n, 0);
    return [`${v.newFiles} new files`, `${nodes} new nodes`, `${v.changedNodes} changed`, `${v.removedNodes} removed`,
      `blast radius ${v.blastRadius?.total ?? 0}`].join(" · ");
  });
</script>

<footer class="checks" aria-label="Checks">
  {#if check}
    <span class="sev error" class:none={!count("error")}>{count("error")} errors</span>
    <span class="sev warning" class:none={!count("warning")}>{count("warning")} warnings</span>
    <span class="volume">{volume}</span>
    <span class="muted">checked in {check.ms} ms</span>
  {:else if error}
    <span class="sev error">Check failed: {error}</span>
  {:else}
    <span class="muted">Checking…</span>
  {/if}
  {#if loose.length}
    <details>
      <summary>{loose.length} not on the board</summary>
      <ul>{#each loose as f (f.fingerprint)}<li><span class="sev {f.severity}">{f.severity}</span> {f.message} <span class="muted">{f.ruleId}</span></li>{/each}</ul>
    </details>
  {/if}
  <span class="status" class:failed role="status">{status}</span>
</footer>

<style>
  .checks { display: flex; gap: 12px; align-items: center; padding: 4px 10px; font-size: 12px; border-top: 1px solid var(--line);
    background: var(--panel); color: var(--fg); min-height: 26px; position: relative; }
  .sev.error { color: var(--error); }
  .sev.warning { color: var(--warning); }
  .sev.note { color: var(--note); }
  .none { color: var(--muted); }
  .muted, .volume { color: var(--muted); }
  details { position: relative; }
  summary { cursor: pointer; color: var(--warning); }
  ul { position: absolute; bottom: 100%; left: 0; margin: 0 0 4px; padding: 6px 10px; list-style: none; min-width: 28em; max-height: 40vh;
    overflow: auto; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; z-index: 3; }
  li { padding: 2px 0; }
  .status { margin-left: auto; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .status.failed { color: var(--error); }
</style>
