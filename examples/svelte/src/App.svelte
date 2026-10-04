<script lang="ts">
  import { onMount } from "svelte";
  let pong = "…";
  onMount(async () => {
    try {
      pong = await window.kitron!.invoke("ping");
    } catch {
      pong = "offline";
    }
  });
</script>

<main style="font-family: system-ui; padding: 32px">
  <h1>Hello Kitron</h1>
  <p>Svelte + Vite — ping → {pong}</p>
</main>
