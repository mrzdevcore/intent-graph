<!-- A button that copies `text` and says so; where the clipboard is blocked it offers the text in a prompt. -->
<script lang="ts">
  let { label, text }: { label: string; text: string } = $props();
  let copied = $state(false);

  $effect(() => {
    text;
    copied = false;
  });

  function copy() {
    (navigator.clipboard ? navigator.clipboard.writeText(text) : Promise.reject()).then(
      () => (copied = true),
      () => window.prompt("Copy:", text),
    );
  }
</script>

<button onclick={copy}>{copied ? "Copied" : label}</button>
