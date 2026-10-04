<script setup lang="ts">
import { onMounted, ref } from "vue";
const pong = ref("…");
onMounted(async () => {
  try {
    pong.value = await window.kitron!.invoke<string>("ping");
  } catch {
    pong.value = "offline";
  }
});
</script>

<template>
  <main style="font-family: system-ui; padding: 32px">
    <h1>Hello Kitron</h1>
    <p>Vue + Vite — ping → {{ pong }}</p>
  </main>
</template>
