#!/usr/bin/env ruby
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# frozen_string_literal: true

# Minimal Ruby client for the headless CMS GraphQL API (stdlib only).
#
#   TESS_URL=http://localhost:8080 ruby cms_client.rb
#
# Shows: list, full-text search, lookup by slug, GraphQL error codes and,
# when TESS_ADMIN_TOKEN is set, creating + publishing a post.

require "json"
require "net/http"
require "uri"

TESS_URL = ENV.fetch("TESS_URL", "http://localhost:8080").chomp("/")
ADMIN_TOKEN = ENV["TESS_ADMIN_TOKEN"] # read from the environment only

class GraphQLError < StandardError
  attr_reader :code, :errors

  def initialize(errors)
    @errors = errors
    @code = errors.first.dig("extensions", "code") || "UNKNOWN"
    super("[#{@code}] #{errors.first['message']}")
  end
end

def gql(query, variables = {}, admin: false)
  uri = URI("#{TESS_URL}/graphql")
  req = Net::HTTP::Post.new(uri, "Content-Type" => "application/json")
  req["Authorization"] = "Bearer #{ADMIN_TOKEN}" if admin
  req.body = JSON.generate(query: query, variables: variables)
  res = Net::HTTP.start(uri.host, uri.port, use_ssl: uri.scheme == "https",
                                           open_timeout: 5, read_timeout: 10) { |http| http.request(req) }
  raise "HTTP #{res.code}: #{res.body}" unless res.is_a?(Net::HTTPSuccess)

  payload = JSON.parse(res.body)
  raise GraphQLError, payload["errors"] if payload["errors"]&.any?

  payload["data"]
end

POST_CARD = "ref title slug excerpt publishedAt author { name }"

LIST_POSTS = <<~GQL
  query ListPosts($first: Int!) {
    posts(orderBy: [createdAt_DESC], first: $first) { total items { #{POST_CARD} } }
  }
GQL

SEARCH_POSTS = <<~GQL
  query Search($q: String!) {
    searchPosts(query: $q, first: 5) { total items { #{POST_CARD} } }
  }
GQL

# Route by slug: `id` is encrypted and expiring, never put it in a URL.
POST_BY_SLUG = <<~GQL
  query PostBySlug($slug: String!) {
    posts(where: { slug: $slug }, first: 1) {
      items { ref title slug body tags publishedAt updatedAt author { name } }
    }
  }
GQL

CREATE_POST = <<~GQL
  mutation Create($data: PostCreateInput!) { createPost(data: $data) { id ref slug stage } }
GQL

PUBLISH_POST = <<~GQL
  mutation Publish($id: ID!) { publishPost(id: $id) { ref slug stage publishedAt } }
GQL

def post_by_slug(slug)
  gql(POST_BY_SLUG, { slug: slug }).dig("posts", "items").first
end

def admin_demo
  unless ADMIN_TOKEN && !ADMIN_TOKEN.empty?
    puts "\n(admin) TESS_ADMIN_TOKEN not set: skipping create/publish"
    return
  end

  slug = "ruby-example-#{Time.now.to_i}-#{Process.pid}"
  data = { title: "Hello from Ruby", slug: slug, excerpt: "Created by examples/clients/ruby.",
           body: "Draft first, then published.", tags: %w[example ruby] }
  created = gql(CREATE_POST, { data: data }, admin: true)["createPost"]
  puts "\n(admin) created draft #{created['slug']} stage=#{created['stage']} ref=#{created['ref']}"
  # The id is used right away and never stored: it is encrypted and expires.
  published = gql(PUBLISH_POST, { id: created["id"] }, admin: true)["publishPost"]
  puts "(admin) published #{published['slug']} stage=#{published['stage']} at #{published['publishedAt']}"
end

begin
  posts = gql(LIST_POSTS, { first: 10 })["posts"]
  puts "#{posts['total']} published post(s):"
  posts["items"].each do |p|
    puts "  - [#{p['ref']}] #{p['title']} (/posts/#{p['slug']}) by #{p.dig('author', 'name') || '?'}"
  end

  hits = gql(SEARCH_POSTS, { q: "lambda" })["searchPosts"]
  puts "\nsearch 'lambda': #{hits['total']} hit(s): #{hits['items'].map { |p| p['slug'] }.inspect}"

  if (first = posts["items"].first)
    post = post_by_slug(first["slug"])
    puts "\nby slug '#{first['slug']}': #{post['title']} tags=#{post['tags'].inspect}\n  #{post['body']}"
  end
  puts "by slug 'does-not-exist': #{post_by_slug('does-not-exist').inspect}"

  begin
    gql("query($id: ID!) { post(id: $id) { title } }", { id: "not-a-real-id" })
  rescue GraphQLError => e
    puts "\nexpected error, code=#{e.code}: #{e.message}"
  end
  begin
    gql(CREATE_POST, { data: { title: "nope" } })
  rescue GraphQLError => e
    puts "expected error, code=#{e.code}: #{e.message}"
  end

  admin_demo
rescue SystemCallError, Net::OpenTimeout, Net::ReadTimeout => e
  warn "cannot reach #{TESS_URL}: #{e.message}"
  exit 1
end
