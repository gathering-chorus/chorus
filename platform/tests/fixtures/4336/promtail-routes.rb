#!/usr/bin/env ruby
# promtail-routes.rb — which scrape jobs would pick up a given log file, per a
# promtail config (#4336). Parses the YAML and, for every static target, applies its
# __path__ glob and __path_exclude__ glob to the file (brace + single-segment *,
# the only glob forms these configs use). Prints one `job` label per matching
# target, sorted. A file two jobs both match is a duplicate Loki stream.
#   ruby promtail-routes.rb <promtail.yaml> <file>
require "yaml"
cfg, file = ARGV
abort("usage: promtail-routes.rb <promtail.yaml> <file>") unless file
flags = File::FNM_PATHNAME | File::FNM_EXTGLOB
jobs = []
(YAML.load_file(cfg)["scrape_configs"] || []).each do |sc|
  (sc["static_configs"] || []).each do |st|
    l = st["labels"] || {}
    inc = l["__path__"] or next
    next unless File.fnmatch(inc, file, flags)
    exc = l["__path_exclude__"]
    next if exc && File.fnmatch(exc, file, flags)
    jobs << (l["job"] || sc["job_name"])
  end
end
puts jobs.sort
