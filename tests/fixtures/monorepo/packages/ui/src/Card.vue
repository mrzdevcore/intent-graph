<template>
  <div class="card"><Button :n="count" @click="bump" /></div>
</template>
<script setup lang="ts">
import Button from './Button';
const count = 1;
const bump = () => count + 1;
</script>
