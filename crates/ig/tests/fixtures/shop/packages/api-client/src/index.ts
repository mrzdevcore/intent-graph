export async function request(path: string, init: { method?: string; body?: unknown } = {}) {
  const res = await fetch(path, { method: init.method ?? 'GET', body: JSON.stringify(init.body) });
  return res.json();
}
