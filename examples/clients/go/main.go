// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

// Command tess-client is a minimal Go (stdlib only) client for the headless
// CMS GraphQL API.
//
//	TESS_URL=http://localhost:8080 go run .
//
// It lists, searches and fetches posts by slug, shows GraphQL error codes and,
// when TESS_ADMIN_TOKEN is set, creates + publishes a post and uploads a file.
package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"
	"time"
)

// Client talks to {BaseURL}/graphql. AdminToken is optional.
type Client struct {
	BaseURL    string
	AdminToken string
	HTTP       *http.Client
}

// GraphQLError is one entry of the response's `errors` array.
type GraphQLError struct {
	Message    string `json:"message"`
	Extensions struct {
		Code string `json:"code"`
	} `json:"extensions"`
}

func (e *GraphQLError) Error() string { return fmt.Sprintf("[%s] %s", e.Extensions.Code, e.Message) }

// Author and Post mirror the demo content model created by scripts/seed.sh.
type Author struct {
	Name string `json:"name"`
}

type Post struct {
	ID          string    `json:"id,omitempty"` // opaque, encrypted, expiring: never persist
	Ref         string    `json:"ref"`          // stable opaque reference: use for keys/caches
	Title       string    `json:"title"`
	Slug        string    `json:"slug"`
	Excerpt     string    `json:"excerpt,omitempty"`
	Body        string    `json:"body,omitempty"`
	Tags        []string  `json:"tags,omitempty"`
	Stage       string    `json:"stage,omitempty"`
	PublishedAt time.Time `json:"publishedAt"`
	Author      *Author   `json:"author"`
}

type PostConnection struct {
	Total int    `json:"total"`
	Items []Post `json:"items"`
}

// Asset is the JSON returned by POST /media/upload.
type Asset struct {
	ID       string `json:"id"`
	Ref      string `json:"ref"`
	FileName string `json:"fileName"`
	MimeType string `json:"mimeType"`
	Size     int64  `json:"size"`
	URL      string `json:"url"`
}

// Do runs a GraphQL operation and decodes `data` into out.
func (c *Client) Do(ctx context.Context, query string, vars map[string]any, admin bool, out any) error {
	body, err := json.Marshal(map[string]any{"query": query, "variables": vars})
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.BaseURL+"/graphql", bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	if admin {
		req.Header.Set("Authorization", "Bearer "+c.AdminToken)
	}
	res, err := c.HTTP.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()
	if res.StatusCode >= 300 {
		b, _ := io.ReadAll(io.LimitReader(res.Body, 4096))
		return fmt.Errorf("HTTP %d: %s", res.StatusCode, b)
	}
	var payload struct {
		Data   json.RawMessage `json:"data"`
		Errors []GraphQLError  `json:"errors"`
	}
	if err := json.NewDecoder(res.Body).Decode(&payload); err != nil {
		return err
	}
	if len(payload.Errors) > 0 {
		return &payload.Errors[0]
	}
	return json.Unmarshal(payload.Data, out)
}

// Upload sends raw bytes to /media/upload (admin only).
func (c *Client) Upload(ctx context.Context, fileName, mimeType string, content []byte) (*Asset, error) {
	u := c.BaseURL + "/media/upload?fileName=" + url.QueryEscape(fileName)
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, u, bytes.NewReader(content))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", mimeType)
	req.Header.Set("Authorization", "Bearer "+c.AdminToken)
	res, err := c.HTTP.Do(req)
	if err != nil {
		return nil, err
	}
	defer res.Body.Close()
	if res.StatusCode >= 300 {
		var e struct{ Error, Code string }
		_ = json.NewDecoder(res.Body).Decode(&e)
		return nil, fmt.Errorf("upload failed (HTTP %d): [%s] %s", res.StatusCode, e.Code, e.Error)
	}
	var a Asset
	if err := json.NewDecoder(res.Body).Decode(&a); err != nil {
		return nil, err
	}
	if strings.HasPrefix(a.URL, "/") { // filesystem store returns a server-relative URL
		a.URL = c.BaseURL + a.URL
	}
	return &a, nil
}

const postCard = `ref title slug excerpt publishedAt author { name }`

const listPosts = `query ListPosts($first: Int!) {
  posts(orderBy: [createdAt_DESC], first: $first) { total items { ` + postCard + ` } }
}`

const searchPosts = `query Search($q: String!) {
  searchPosts(query: $q, first: 5) { total items { ` + postCard + ` } }
}`

// Route by slug, not id: ids are encrypted and expire.
const postBySlug = `query PostBySlug($slug: String!) {
  posts(where: { slug: $slug }, first: 1) {
    items { ref title slug body tags publishedAt author { name } }
  }
}`

const createPost = `mutation Create($data: PostCreateInput!) { createPost(data: $data) { id ref slug stage } }`

const publishPost = `mutation Publish($id: ID!) { publishPost(id: $id) { ref slug stage publishedAt } }`

func (c *Client) PostBySlug(ctx context.Context, slug string) (*Post, error) {
	var d struct{ Posts PostConnection }
	if err := c.Do(ctx, postBySlug, map[string]any{"slug": slug}, false, &d); err != nil {
		return nil, err
	}
	if len(d.Posts.Items) == 0 {
		return nil, nil
	}
	return &d.Posts.Items[0], nil
}

func authorName(p Post) string {
	if p.Author == nil {
		return "?"
	}
	return p.Author.Name
}

func run(ctx context.Context, c *Client) error {
	var list struct{ Posts PostConnection }
	if err := c.Do(ctx, listPosts, map[string]any{"first": 10}, false, &list); err != nil {
		return err
	}
	fmt.Printf("%d published post(s):\n", list.Posts.Total)
	for _, p := range list.Posts.Items {
		fmt.Printf("  - [%s] %s (/posts/%s) by %s\n", p.Ref, p.Title, p.Slug, authorName(p))
	}

	var search struct{ SearchPosts PostConnection }
	if err := c.Do(ctx, searchPosts, map[string]any{"q": "lambda"}, false, &search); err != nil {
		return err
	}
	slugs := make([]string, 0, len(search.SearchPosts.Items))
	for _, p := range search.SearchPosts.Items {
		slugs = append(slugs, p.Slug)
	}
	fmt.Printf("\nsearch 'lambda': %d hit(s): %v\n", search.SearchPosts.Total, slugs)

	if len(list.Posts.Items) > 0 {
		slug := list.Posts.Items[0].Slug
		p, err := c.PostBySlug(ctx, slug)
		if err != nil {
			return err
		}
		fmt.Printf("\nby slug '%s': %s tags=%v\n  %s\n", slug, p.Title, p.Tags, p.Body)
	}
	if p, err := c.PostBySlug(ctx, "does-not-exist"); err == nil && p == nil {
		fmt.Println("by slug 'does-not-exist': not found")
	}

	// Error handling: inspect extensions.code.
	var gqlErr *GraphQLError
	var ignored any
	err := c.Do(ctx, `query($id: ID!) { post(id: $id) { title } }`, map[string]any{"id": "not-a-real-id"}, false, &ignored)
	if errors.As(err, &gqlErr) {
		fmt.Printf("\nexpected error, code=%s: %v\n", gqlErr.Extensions.Code, gqlErr)
	}
	err = c.Do(ctx, createPost, map[string]any{"data": map[string]any{"title": "nope"}}, false, &ignored)
	if errors.As(err, &gqlErr) {
		fmt.Printf("expected error, code=%s: %v\n", gqlErr.Extensions.Code, gqlErr)
	}

	return adminDemo(ctx, c)
}

func adminDemo(ctx context.Context, c *Client) error {
	if c.AdminToken == "" {
		fmt.Println("\n(admin) TESS_ADMIN_TOKEN not set: skipping create/publish and media upload")
		return nil
	}
	slug := fmt.Sprintf("go-example-%d", time.Now().UnixNano())
	var created struct{ CreatePost Post }
	data := map[string]any{
		"title": "Hello from Go", "slug": slug, "excerpt": "Created by examples/clients/go.",
		"body": "Draft first, then published.", "tags": []string{"example", "go"},
	}
	if err := c.Do(ctx, createPost, map[string]any{"data": data}, true, &created); err != nil {
		return err
	}
	fmt.Printf("\n(admin) created draft %s stage=%s ref=%s\n", created.CreatePost.Slug, created.CreatePost.Stage, created.CreatePost.Ref)
	// The id is used immediately and never stored.
	var published struct{ PublishPost Post }
	if err := c.Do(ctx, publishPost, map[string]any{"id": created.CreatePost.ID}, true, &published); err != nil {
		return err
	}
	fmt.Printf("(admin) published %s stage=%s at %s\n", published.PublishPost.Slug, published.PublishPost.Stage,
		published.PublishPost.PublishedAt.Format(time.RFC3339))

	content := []byte("generated by the go example at " + time.Now().Format(time.RFC3339) + "\n")
	asset, err := c.Upload(ctx, "go-example.txt", "text/plain", content)
	if err != nil {
		return err
	}
	fmt.Printf("(admin) uploaded %s (%s, %d bytes) ref=%s\n(admin) download: %s\n",
		asset.FileName, asset.MimeType, asset.Size, asset.Ref, asset.URL)
	return nil
}

func main() {
	base := strings.TrimRight(os.Getenv("TESS_URL"), "/")
	if base == "" {
		base = "http://localhost:8080"
	}
	c := &Client{BaseURL: base, AdminToken: os.Getenv("TESS_ADMIN_TOKEN"), HTTP: &http.Client{Timeout: 15 * time.Second}}
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	if err := run(ctx, c); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}
