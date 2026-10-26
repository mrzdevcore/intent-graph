// Rebuild events can arrive in bursts: run the job at most once at a time, and once more after it
// for whatever arrived meanwhile (any number of triggers collapse into that one pending run).

export function coalesce(job: () => Promise<unknown>): () => void {
  let running = false;
  let pending = false;
  const run = () => {
    running = true;
    job().catch(() => {}).finally(() => {
      running = false;
      if (pending) {
        pending = false;
        run();
      }
    });
  };
  return () => (running ? (pending = true) : run());
}
