#!/usr/bin/env ruby
# gha-step.rb — print one workflow step's `run:` script, with its ${{ expr }}
# placeholders filled from expr=value arguments, so a test can EXECUTE the step
# the workflow runs instead of grepping its text (#4336).
#   ruby gha-step.rb <workflow.yml> <job> <step-name> [expr=value ...]
# An expression with no value given is a hard error (exit 3): a step must never
# run with a placeholder silently left in or blanked out.
require "yaml"
yml, job, name, *pairs = ARGV
abort("usage: gha-step.rb <yml> <job> <step> [expr=value ...]") unless name
vals = {}
pairs.each { |p| k, v = p.split("=", 2); vals[k] = v.to_s }
steps = (YAML.load_file(yml).dig("jobs", job, "steps") || [])
step = steps.find { |s| s["name"] == name }
unless step && step["run"]
  warn "gha-step: no step '#{name}' with a run: block in job '#{job}' of #{yml}"
  exit 2
end
missing = []
out = step["run"].gsub(/\$\{\{\s*([^}]+?)\s*\}\}/) do
  k = Regexp.last_match(1)
  missing << k unless vals.key?(k)
  vals[k].to_s
end
unless missing.empty?
  warn "gha-step: no value given for: #{missing.uniq.join(', ')}"
  exit 3
end
print out
