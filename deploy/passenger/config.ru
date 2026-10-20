# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Tess behind Phusion Passenger (hosts that only run Ruby/Node/Python apps).
#
# Passenger serves the prebuilt admin UI straight from `public/admin/` and
# hands every other request to this Rack app, which:
#   1. starts the `tess` binary on 127.0.0.1 and restarts it if it exits;
#   2. proxies requests to it with rack-proxy.
# Requests for admin routes that are not files (client-side routes, folders
# without a trailing slash) are proxied too: tess serves them from the same
# `public/admin` folder with its SPA fallback.
#
# Required Passenger settings: one process, never stopped when idle, direct
# spawning. See docs/docs/devops/passenger.md.

require "rack/proxy"
require "socket"
require "fileutils"

ROOT = __dir__
BIN  = ENV.fetch("TESS_BIN", File.join(ROOT, "tess"))
HOST = "127.0.0.1"
PORT = Integer(ENV.fetch("TESS_INTERNAL_PORT", "18080"))
LOG  = ENV.fetch("TESS_LOG_FILE", File.join(ROOT, "log", "tess.log"))

FileUtils.mkdir_p([File.dirname(LOG), File.join(ROOT, "tmp")])

module TessProcess
  module_function

  def up?
    Socket.tcp(HOST, PORT, connect_timeout: 0.5) { true }
  rescue SystemCallError, SocketError
    false
  end

  def env
    {
      "PORT" => PORT.to_s,
      # Same folder Passenger serves statically, for the SPA fallback.
      "TESS_ADMIN_DIR" => ENV.fetch("TESS_ADMIN_DIR", File.join(ROOT, "public", "admin")),
    }
  end

  # Starts tess and keeps it running for the life of this app process. The
  # file lock makes sure only one Passenger process supervises tess, even if
  # the pool is (wrongly) configured with several.
  def supervise
    lock = File.open(File.join(ROOT, "tmp", "tess.lock"), File::RDWR | File::CREAT, 0o600)
    return unless lock.flock(File::LOCK_EX | File::LOCK_NB)
    raise "tess binary not found or not executable: #{BIN}" unless File.executable?(BIN)

    @lock = lock
    @pid = nil
    @stopping = false
    at_exit { stop }
    Thread.new do
      until @stopping
        @pid = Process.spawn(env, BIN, chdir: ROOT, out: [LOG, "a"], err: [:child, :out], pgroup: true)
        Process.wait(@pid)
        break if @stopping
        warn "[tess] exited with #{$?.exitstatus.inspect}, restarting in 2s"
        sleep 2
      end
    end
  end

  # Stops tess with the app process (Passenger restart, shutdown).
  def stop
    @stopping = true
    return unless @pid
    Process.kill("TERM", @pid)
    50.times do
      return if Process.waitpid(@pid, Process::WNOHANG)
      sleep 0.1
    end
    Process.kill("KILL", @pid)
  rescue Errno::ESRCH, Errno::ECHILD
    nil
  end

  def wait_until_up(seconds)
    (seconds * 10).times do
      return true if up?
      sleep 0.1
    end
    false
  end
end

TessProcess.supervise unless TessProcess.up?
TessProcess.wait_until_up(30) or warn "[tess] not reachable on #{HOST}:#{PORT} yet, see #{LOG}"

# Hop-by-hop headers must not be forwarded (RFC 9110 §7.6.1); Rack sets its own.
HOP_HEADERS = %w[connection keep-alive proxy-connection transfer-encoding te trailer upgrade status].freeze

class TessProxy < Rack::Proxy
  def perform_request(env)
    env["HTTP_X_FORWARDED_HOST"] ||= env["HTTP_HOST"]
    env["HTTP_X_FORWARDED_PROTO"] ||= env["rack.url_scheme"]
    env["HTTP_X_FORWARDED_FOR"] = [env["HTTP_X_FORWARDED_FOR"], env["REMOTE_ADDR"]].compact.join(", ")
    status, headers, body = super
    # rack-proxy returns Net::HTTP values: a String status and mixed-case,
    # possibly multi-valued headers. Rack 3 wants an Integer and lowercase names.
    clean = {}
    headers.each do |name, value|
      key = name.to_s.downcase
      next if HOP_HEADERS.include?(key)
      value = Array(value)
      clean[key] = key == "set-cookie" ? value : value.join(", ")
    end
    [Integer(status), clean, body]
  rescue Errno::ECONNREFUSED, Errno::ECONNRESET, Net::ReadTimeout, Net::OpenTimeout
    [502, { "content-type" => "application/json" }, ['{"error":"tess is starting or unavailable"}']]
  end
end

run TessProxy.new(backend: "http://#{HOST}:#{PORT}", streaming: false, read_timeout: 300)
