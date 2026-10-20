<?php
// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

/**
 * Minimal PHP 8 client for the headless CMS GraphQL API (ext-curl + ext-json).
 *
 *   TESS_URL=http://localhost:8080 php cms_client.php
 *
 * Lists, searches and fetches posts by slug, shows GraphQL error codes and,
 * when TESS_ADMIN_TOKEN is set, creates + publishes a post and uploads a file.
 */
declare(strict_types=1);

final class GraphQLException extends RuntimeException
{
    public function __construct(public readonly string $errorCode, string $message)
    {
        parent::__construct("[$errorCode] $message");
    }
}

final class CmsClient
{
    public function __construct(
        private readonly string $baseUrl,
        private readonly ?string $adminToken = null,
    ) {
    }

    public function hasAdminToken(): bool
    {
        return $this->adminToken !== null && $this->adminToken !== '';
    }

    /** Sends a POST and returns [status, body]. */
    private function post(string $url, string $body, array $headers): array
    {
        $ch = curl_init($url);
        curl_setopt_array($ch, [
            CURLOPT_POST => true,
            CURLOPT_POSTFIELDS => $body,
            CURLOPT_HTTPHEADER => $headers,
            CURLOPT_RETURNTRANSFER => true,
            CURLOPT_CONNECTTIMEOUT => 5,
            CURLOPT_TIMEOUT => 15,
        ]);
        $response = curl_exec($ch);
        if ($response === false) {
            $error = curl_error($ch);
            curl_close($ch);
            throw new RuntimeException("cannot reach {$this->baseUrl}: $error");
        }
        $status = (int) curl_getinfo($ch, CURLINFO_RESPONSE_CODE);
        curl_close($ch);
        return [$status, (string) $response];
    }

    /** Runs a GraphQL operation and returns its `data`. */
    public function query(string $query, array $variables = [], bool $admin = false): array
    {
        $headers = ['Content-Type: application/json'];
        if ($admin) {
            $headers[] = 'Authorization: Bearer ' . $this->adminToken;
        }
        $body = json_encode(['query' => $query, 'variables' => (object) $variables], JSON_THROW_ON_ERROR);
        [$status, $raw] = $this->post($this->baseUrl . '/graphql', $body, $headers);
        if ($status >= 300) {
            throw new RuntimeException("HTTP $status: $raw");
        }
        $payload = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
        if (!empty($payload['errors'])) {
            $first = $payload['errors'][0];
            throw new GraphQLException($first['extensions']['code'] ?? 'UNKNOWN', $first['message'] ?? 'error');
        }
        return $payload['data'];
    }

    /** Uploads raw bytes to /media/upload (admin only). */
    public function upload(string $fileName, string $mimeType, string $content): array
    {
        $url = $this->baseUrl . '/media/upload?' . http_build_query(['fileName' => $fileName]);
        [$status, $raw] = $this->post($url, $content, [
            'Content-Type: ' . $mimeType,
            'Authorization: Bearer ' . $this->adminToken,
        ]);
        $asset = json_decode($raw, true) ?? [];
        if ($status >= 300) {
            throw new RuntimeException(sprintf('upload failed (HTTP %d): [%s] %s', $status, $asset['code'] ?? '?', $asset['error'] ?? $raw));
        }
        if (str_starts_with($asset['url'], '/')) {
            $asset['url'] = $this->baseUrl . $asset['url']; // filesystem store: server-relative URL
        }
        return $asset;
    }
}

const POST_CARD = 'ref title slug excerpt publishedAt author { name }';

const LIST_POSTS = 'query ListPosts($first: Int!) {
  posts(orderBy: [createdAt_DESC], first: $first) { total items { ' . POST_CARD . ' } }
}';

const SEARCH_POSTS = 'query Search($q: String!) {
  searchPosts(query: $q, first: 5) { total items { ' . POST_CARD . ' } }
}';

// Route by slug: ids are encrypted and expire, never put them in URLs.
const POST_BY_SLUG = 'query PostBySlug($slug: String!) {
  posts(where: { slug: $slug }, first: 1) {
    items { ref title slug body tags publishedAt updatedAt author { name } }
  }
}';

const CREATE_POST = 'mutation Create($data: PostCreateInput!) { createPost(data: $data) { id ref slug stage } }';
const PUBLISH_POST = 'mutation Publish($id: ID!) { publishPost(id: $id) { ref slug stage publishedAt } }';

function postBySlug(CmsClient $cms, string $slug): ?array
{
    return $cms->query(POST_BY_SLUG, ['slug' => $slug])['posts']['items'][0] ?? null;
}

function adminDemo(CmsClient $cms): void
{
    if (!$cms->hasAdminToken()) {
        echo "\n(admin) TESS_ADMIN_TOKEN not set: skipping create/publish and media upload\n";
        return;
    }
    $slug = 'php-example-' . time() . '-' . getmypid();
    $created = $cms->query(CREATE_POST, ['data' => [
        'title' => 'Hello from PHP',
        'slug' => $slug,
        'excerpt' => 'Created by examples/clients/php.',
        'body' => 'Draft first, then published.',
        'tags' => ['example', 'php'],
    ]], true)['createPost'];
    printf("\n(admin) created draft %s stage=%s ref=%s\n", $created['slug'], $created['stage'], $created['ref']);
    // The id is used immediately and never stored.
    $published = $cms->query(PUBLISH_POST, ['id' => $created['id']], true)['publishPost'];
    printf("(admin) published %s stage=%s at %s\n", $published['slug'], $published['stage'], $published['publishedAt']);

    $asset = $cms->upload('php-example.txt', 'text/plain', 'generated by the php example at ' . date(DATE_ATOM) . "\n");
    printf("(admin) uploaded %s (%s, %d bytes) ref=%s\n(admin) download: %s\n",
        $asset['fileName'], $asset['mimeType'], $asset['size'], $asset['ref'], $asset['url']);
}

$cms = new CmsClient(
    rtrim(getenv('TESS_URL') ?: 'http://localhost:8080', '/'),
    getenv('TESS_ADMIN_TOKEN') ?: null, // read from the environment only
);

try {
    $posts = $cms->query(LIST_POSTS, ['first' => 10])['posts'];
    printf("%d published post(s):\n", $posts['total']);
    foreach ($posts['items'] as $p) {
        printf("  - [%s] %s (/posts/%s) by %s\n", $p['ref'], $p['title'], $p['slug'], $p['author']['name'] ?? '?');
    }

    $hits = $cms->query(SEARCH_POSTS, ['q' => 'lambda'])['searchPosts'];
    printf("\nsearch 'lambda': %d hit(s): %s\n", $hits['total'], implode(', ', array_column($hits['items'], 'slug')));

    if ($posts['items']) {
        $slug = $posts['items'][0]['slug'];
        $post = postBySlug($cms, $slug);
        printf("\nby slug '%s': %s tags=%s\n  %s\n", $slug, $post['title'], json_encode($post['tags']), $post['body']);
    }
    printf("by slug 'does-not-exist': %s\n", var_export(postBySlug($cms, 'does-not-exist'), true));

    foreach ([
        ['query($id: ID!) { post(id: $id) { title } }', ['id' => 'not-a-real-id']],
        [CREATE_POST, ['data' => ['title' => 'nope']]],
    ] as [$q, $vars]) {
        try {
            $cms->query($q, $vars);
        } catch (GraphQLException $e) {
            printf("expected error, code=%s: %s\n", $e->errorCode, $e->getMessage());
        }
    }

    adminDemo($cms);
} catch (Throwable $e) {
    fwrite(STDERR, 'error: ' . $e->getMessage() . "\n");
    exit(1);
}
