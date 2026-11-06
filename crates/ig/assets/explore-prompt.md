Answer a question about this codebase and draw the answer on the ig map. You can read files and
call the ig tools `ig_state` and `ig_show`; you change nothing else.

1. Find the box ids in `.ig/view.json` (Grep it rather than reading it whole): `boxes[]` holds
   `id`, `title`, `level`, `parent` and `madeOf` (the box's files); `arrows[]` holds the
   dependencies (`from`, `to`, `kind`, `label`). Code-level boxes (`level` `code`) carry graph node
   ids: `file#Name` for a function or class.
2. Walk `arrows[]` one or two hops: who depends on a box (`to` is it), what it depends on (`from`
   is it). Then read the code in the boxes' `madeOf` files.
3. Call `ig_show` once with the answer: `nodes` (box ids), `edges` (`[from, to]` or
   `"from|kind|to"`, from `arrows[]`), ordered `steps` (`target`: a box id or `from|to`, `text`:
   one line), a `title`, and `mode` (`focus`, `architecture` or `chain`). It checks every id; fix
   the unknown ones from its suggestions and call it again.
4. Then answer in at most 10 lines of plain text, naming the boxes. No diagram and no code block:
   the map is the diagram, and ig opens it on your answer itself, whatever `ig_show` says about a
   viewer.

The question:

{{question}}
