#!/usr/bin/env ruby
# @domain: logs
# promtail-audit.rb (#4994 AC1) — reads a promtail config and the files on disk its
# targets would tail. Prints one line per finding:
#   DOUBLE <file> <job>,<job>…   a file two or more targets tail (a duplicate Loki stream)
#   DEAD <job> <glob>            a target whose __path__ matches no file on disk
# Same glob rules as fixtures/4336/promtail-routes.rb (braces + single-segment *).
# ROOT_PREFIX re-roots every glob (a fixture tree for the negative proof).
#   ruby promtail-audit.rb <promtail.yaml>
require "yaml"
cfg = ARGV[0] or abort("usage: promtail-audit.rb <promtail.yaml>")
root = ENV.fetch("ROOT_PREFIX", "")
flags = File::FNM_PATHNAME | File::FNM_EXTGLOB
seen = Hash.new { |h, k| h[k] = [] }
dead = []
(YAML.load_file(cfg)["scrape_configs"] || []).each do |sc|
  (sc["static_configs"] || []).each do |st|
    l = st["labels"] || {}
    inc = l["__path__"] or next
    job = l["job"] || sc["job_name"]
    exc = l["__path_exclude__"]
    files = Dir.glob(root + inc, File::FNM_EXTGLOB).select { |f| File.file?(f) }
    files.reject! { |f| exc && File.fnmatch(root + exc, f, flags) }
    dead << "DEAD #{job} #{inc}" if files.empty?
    files.each { |f| seen[f.delete_prefix(root)] << job }
  end
end
seen.sort.each { |f, jobs| puts "DOUBLE #{f} #{jobs.sort.join(',')}" if jobs.size > 1 }
puts dead.sort
