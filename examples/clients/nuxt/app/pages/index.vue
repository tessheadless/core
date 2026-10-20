<!--
  Copyright © 2026 Tess Dev Team <https://tessheadless.com>
  SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
  Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.
-->

<script setup lang="ts">
import type { PostConnection } from "#shared/types/cms";

const q = ref("");
const search = ref("");
// Fetched on the server during SSR/prerender, then hydrated from the payload.
const { data, error } = await useAsyncData("posts", () => $fetch<PostConnection>("/api/posts"));
// Client-side search through the server route (which calls searchPosts).
const { data: hits, status } = await useAsyncData(
  "search",
  () => (search.value ? $fetch<PostConnection>("/api/posts", { query: { q: search.value } }) : Promise.resolve(null)),
  { watch: [search], server: false },
);
useHead({ title: "CMS blog (Nuxt)" });
</script>

<template>
  <main>
    <h1>Posts ({{ data?.total ?? 0 }})</h1>
    <p v-if="error">Could not load posts: {{ error.message }}</p>
    <form @submit.prevent="search = q.trim()">
      <input v-model="q" placeholder="Search posts" />
      <button type="submit">Search</button>
    </form>
    <section v-if="search">
      <p v-if="status === 'pending'">Searching…</p>
      <p v-else>{{ hits?.total ?? 0 }} result(s) for “{{ search }}”</p>
      <ul>
        <li v-for="p in hits?.items ?? []" :key="p.ref">
          <NuxtLink :to="`/posts/${p.slug}`">{{ p.title }}</NuxtLink>
        </li>
      </ul>
    </section>
    <ul>
      <!-- `ref` is the stable key; links use the slug. -->
      <li v-for="p in data?.items ?? []" :key="p.ref">
        <NuxtLink :to="`/posts/${p.slug}`">{{ p.title }}</NuxtLink>
        <span v-if="p.author"> by {{ p.author.name }}</span>
        <p v-if="p.excerpt">{{ p.excerpt }}</p>
      </li>
    </ul>
  </main>
</template>
