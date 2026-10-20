<!--
  Copyright © 2026 Tess Dev Team <https://tessheadless.com>
  SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
  Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.
-->

<script setup lang="ts">
import type { Post } from "#shared/types/cms";

const route = useRoute();
const slug = computed(() => String(route.params.slug));
const { data: post, error } = await useAsyncData(
  () => `post:${slug.value}`,
  () => $fetch<Post>(`/api/posts/${encodeURIComponent(slug.value)}`),
);
if (error.value) {
  throw createError({ statusCode: error.value.statusCode ?? 500, statusMessage: error.value.statusMessage, fatal: true });
}
useHead({ title: () => post.value?.title ?? "Post" });
</script>

<template>
  <article v-if="post">
    <h1>{{ post.title }}</h1>
    <p>
      {{ post.author?.name ?? "Unknown author" }}
      <template v-if="post.publishedAt"> · {{ new Date(post.publishedAt).toLocaleDateString("en") }}</template>
    </p>
    <p v-if="post.tags?.length">Tags: {{ post.tags.join(", ") }}</p>
    <div style="white-space: pre-wrap">{{ post.body }}</div>
  </article>
</template>
