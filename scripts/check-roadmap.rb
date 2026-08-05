#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "yaml"

ROOT = File.expand_path("..", __dir__)
ROADMAP = File.join(ROOT, "TODO.md")
EXPECTED_TASK_FILES = %w[
  todo/M00-M02-foundation.md
  todo/M03-M05-community.md
  todo/M06-M07-storage-economy.md
  todo/M08-M12-integrations.md
  todo/M13-M17-release.md
].map { |path| File.join(ROOT, path) }.freeze
DISCOVERED_TASK_FILES = Dir[File.join(ROOT, "todo", "M*.md")].sort.freeze
COVERAGE_PATH = File.join(ROOT, "todo", "openapi-operation-coverage.json")
OPENAPI_PATH = File.join(ROOT, "openapi", "openapi.yaml")
EXPECTED_MILESTONES = (0..17).map { |number| "M#{number}" }.freeze
MILESTONE_FILES = {
  (0..2) => "todo/M00-M02-foundation.md",
  (3..5) => "todo/M03-M05-community.md",
  (6..7) => "todo/M06-M07-storage-economy.md",
  (8..12) => "todo/M08-M12-integrations.md",
  (13..17) => "todo/M13-M17-release.md"
}.freeze
HTTP_METHODS = %w[get post put patch delete head options trace].freeze
ALLOWED_TASK_STATES = [" ", "x", "~", "!"].freeze
ALLOWED_IMPLEMENTATION_STATES = %w[not_started baseline_only in_progress implemented verified blocked].freeze
REQUIRED_EVIDENCE_FIELDS = %w[files commands contract commit review].freeze
REQUIRED_COVERAGE_FIELDS = %w[
  operation_id method path primary_tag milestone work_package priority
  contract_status implementation_status owner handler tests evidence
].freeze


def relative(path)
  path.delete_prefix(ROOT + "/")
end


def milestone_file(number)
  pair = MILESTONE_FILES.find { |range, _path| range.include?(number) }
  pair && pair[1]
end


def assigned_owner?(owner)
  owner.is_a?(String) && !owner.empty? && owner != "unassigned" && !owner.start_with?("unassigned/")
end


def derived_status(tasks)
  return "未开始" if tasks.empty?
  return "阻塞" if tasks.any? { |task| task.fetch(:state) == "!" }
  return "完成" if tasks.all? { |task| task.fetch(:state) == "x" }
  return "进行中" if tasks.any? { |task| %w[x ~].include?(task.fetch(:state)) }

  "未开始"
end

errors = []
roadmap = File.file?(ROADMAP) ? File.read(ROADMAP) : ""

if roadmap.empty?
  errors << "Missing or empty TODO.md"
else
  errors << "TODO.md must declare roadmap version v1.0.0-rc.2" unless roadmap.include?("路线图版本：v1.0.0-rc.2")
end

missing_task_files = EXPECTED_TASK_FILES.reject { |file| File.file?(file) }
missing_task_files.each { |file| errors << "Missing milestone task file #{relative(file)}" }
extra_task_files = DISCOVERED_TASK_FILES - EXPECTED_TASK_FILES
extra_task_files.each { |file| errors << "Unexpected milestone task file #{relative(file)}; register it explicitly before use" }
TASK_FILES = EXPECTED_TASK_FILES.select { |file| File.file?(file) }.freeze

package_ids = {}
packages = {}
leaf_ids = {}
leaf_tasks = []
status_counts = Hash.new(0)
priority_counts = Hash.new(0)
milestone_headings = {}
anchors_by_file = {}

TASK_FILES.each do |file|
  lines = File.readlines(file, chomp: true)
  anchors = Hash.new { |hash, key| hash[key] = [] }
  lines.each_with_index do |line, index|
    match = line.match(/<a\s+id="([a-z0-9-]+)"\s*><\/a>/)
    anchors[match[1]] << index + 1 if match
  end
  anchors.each do |anchor, locations|
    errors << "#{relative(file)}: duplicate anchor ##{anchor} at lines #{locations.join(', ')}" if locations.length > 1
  end
  anchors_by_file[file] = anchors

  current_milestone = nil
  current_package = nil
  current_package_priority = nil

  lines.each_with_index do |line, index|
    line_number = index + 1

    if (match = line.match(/^# M(\d+)：/))
      current_milestone = "M#{match[1].to_i}"
      current_package = nil
      current_package_priority = nil
      if milestone_headings.key?(current_milestone)
        errors << "#{relative(file)}:#{line_number}: duplicate milestone #{current_milestone}"
      else
        milestone_headings[current_milestone] = { file: file, line: line_number }
      end

      expected_file = milestone_file(match[1].to_i)
      errors << "#{relative(file)}:#{line_number}: #{current_milestone} belongs in #{expected_file}" unless relative(file) == expected_file
      anchor = current_milestone.downcase
      errors << "#{relative(file)}:#{line_number}: #{current_milestone} is missing explicit anchor ##{anchor}" unless anchors.key?(anchor)
      next
    end

    if (match = line.match(/^## (M\d{2}-[A-Z0-9-]+)：/))
      current_package = match[1]
      package_milestone = "M#{current_package[/\AM(\d{2})-/, 1].to_i}"
      if current_milestone != package_milestone
        errors << "#{relative(file)}:#{line_number}: #{current_package} is under #{current_milestone || 'no milestone'}, expected #{package_milestone}"
      end

      if package_ids.key?(current_package)
        errors << "#{relative(file)}:#{line_number}: duplicate work package #{current_package} (first at #{package_ids[current_package]})"
      else
        package_ids[current_package] = "#{relative(file)}:#{line_number}"
      end

      metadata_window = lines[index + 1, 8] || []
      metadata = metadata_window.find { |candidate| candidate.start_with?("**元数据：**") }
      target = metadata_window.find { |candidate| candidate.start_with?("**目标文件：**") }
      acceptance = metadata_window.find { |candidate| candidate.start_with?("**验收：**") }
      owner = nil
      risk = nil
      dependencies = []
      blocked = nil

      unless metadata
        errors << "#{relative(file)}:#{line_number}: #{current_package} is missing metadata"
        current_package_priority = nil
      else
        current_package_priority = metadata[/`(P[012])`/, 1]
        owner = metadata[/owner=([^`\s]+)/, 1]
        risk = metadata[/risk=([^`\s]+)/, 1]
        dependency_text = metadata[/depends=([^`]+)/, 1]
        blocked = metadata[/blocked=([^`]+)/, 1]
        dependencies = dependency_text.to_s.split(",").map(&:strip).reject(&:empty?)

        errors << "#{relative(file)}:#{line_number}: #{current_package} metadata is missing P0/P1/P2" unless current_package_priority
        errors << "#{relative(file)}:#{line_number}: #{current_package} metadata has an invalid owner" unless owner&.match?(/\A[a-z0-9._@\/-]+\z/)
        errors << "#{relative(file)}:#{line_number}: #{current_package} metadata has invalid risk #{risk.inspect}" unless %w[critical high medium low].include?(risk)
        errors << "#{relative(file)}:#{line_number}: #{current_package} metadata is missing depends" if dependency_text.nil? || dependencies.empty?
        errors << "#{relative(file)}:#{line_number}: #{current_package} metadata is missing blocked" if blocked.nil? || blocked.empty?
      end

      errors << "#{relative(file)}:#{line_number}: #{current_package} is missing target files" unless target && target.length > "**目标文件：**".length
      errors << "#{relative(file)}:#{line_number}: #{current_package} is missing acceptance command/result" unless acceptance && acceptance.length > "**验收：**".length

      packages[current_package] = {
        id: current_package,
        file: file,
        line: line_number,
        milestone: package_milestone,
        priority: current_package_priority,
        owner: owner,
        risk: risk,
        dependencies: dependencies,
        blocked: blocked,
        tasks: []
      }
      next
    end

    task_match = line.match(/^\s*- \[([^\]])\]\s+`([^`]+)`(.*)$/)
    next unless task_match

    state = task_match[1]
    task_id = task_match[2]
    remainder = task_match[3]
    status_counts[state] += 1

    unless ALLOWED_TASK_STATES.include?(state)
      errors << "#{relative(file)}:#{line_number}: invalid task state [#{state}] for #{task_id}"
    end
    unless current_package && packages.key?(current_package)
      errors << "#{relative(file)}:#{line_number}: task #{task_id} is outside a work package"
      next
    end
    unless task_id.start_with?(current_package + "-")
      errors << "#{relative(file)}:#{line_number}: task #{task_id} does not belong to current package #{current_package}"
    end
    if leaf_ids.key?(task_id)
      errors << "#{relative(file)}:#{line_number}: duplicate task #{task_id} (first at #{leaf_ids[task_id]})"
    else
      leaf_ids[task_id] = "#{relative(file)}:#{line_number}"
    end

    estimate = remainder[/`\[(\d+)m\]`/, 1]
    if estimate.nil?
      errors << "#{relative(file)}:#{line_number}: task #{task_id} is missing an [Nm] estimate"
    elsif !estimate.to_i.between?(15, 60)
      errors << "#{relative(file)}:#{line_number}: task #{task_id} estimate #{estimate}m is outside 15-60m"
    end

    explicit_priority = remainder[/`(P[012])`/, 1]
    effective_priority = explicit_priority || current_package_priority
    if effective_priority
      priority_counts[effective_priority] += 1
    else
      errors << "#{relative(file)}:#{line_number}: task #{task_id} has no effective priority"
    end

    if %w[x ~ !].include?(state) && !assigned_owner?(packages.fetch(current_package).fetch(:owner))
      errors << "#{relative(file)}:#{line_number}: active/completed task #{task_id} requires an assigned work-package owner"
    end

    if state == "x"
      evidence = remainder.split("证据：", 2)[1]
      if evidence.nil?
        errors << "#{relative(file)}:#{line_number}: completed task #{task_id} is missing inline evidence"
      else
        REQUIRED_EVIDENCE_FIELDS.each do |field|
          value = evidence[/#{Regexp.escape(field)}=([^；\n]+)/, 1]
          errors << "#{relative(file)}:#{line_number}: completed task #{task_id} evidence is missing #{field}=..." if value.nil? || value.strip.empty?
        end
      end
    end

    if state == "!"
      blocker = remainder.split("阻塞：", 2)[1]
      if blocker.nil?
        errors << "#{relative(file)}:#{line_number}: blocked task #{task_id} is missing blocker metadata"
      else
        %w[负责人 复查日期 解除条件].each do |field|
          errors << "#{relative(file)}:#{line_number}: blocked task #{task_id} is missing #{field}" unless blocker.include?(field)
        end
      end
    end

    task = {
      id: task_id,
      file: file,
      line: line_number,
      package: current_package,
      milestone: packages.fetch(current_package).fetch(:milestone),
      state: state,
      priority: effective_priority
    }
    leaf_tasks << task
    packages.fetch(current_package).fetch(:tasks) << task
  end
end

missing_milestones = EXPECTED_MILESTONES - milestone_headings.keys
missing_milestones.each { |milestone| errors << "Missing milestone heading #{milestone}" }
extra_milestones = milestone_headings.keys - EXPECTED_MILESTONES
extra_milestones.each { |milestone| errors << "Unexpected milestone heading #{milestone}" }

packages.each_value do |package|
  errors << "#{relative(package.fetch(:file))}:#{package.fetch(:line)}: #{package.fetch(:id)} has no leaf tasks" if package.fetch(:tasks).empty?

  blocked_tasks = package.fetch(:tasks).select { |task| task.fetch(:state) == "!" }
  if blocked_tasks.any? && package.fetch(:blocked) == "none"
    errors << "#{relative(package.fetch(:file))}:#{package.fetch(:line)}: #{package.fetch(:id)} has blocked tasks but blocked=none"
  elsif blocked_tasks.empty? && package.fetch(:blocked) && package.fetch(:blocked) != "none"
    errors << "#{relative(package.fetch(:file))}:#{package.fetch(:line)}: #{package.fetch(:id)} declares a blocker but no leaf task is [!]"
  end

  package.fetch(:dependencies).each do |dependency|
    next if dependency == "none" || dependency.match?(/\ABASE-\d{3}\z/)

    errors << "#{relative(package.fetch(:file))}:#{package.fetch(:line)}: #{package.fetch(:id)} has unknown dependency #{dependency}" unless packages.key?(dependency)
    errors << "#{relative(package.fetch(:file))}:#{package.fetch(:line)}: #{package.fetch(:id)} cannot depend on itself" if dependency == package.fetch(:id)
  end
end

visit_state = {}
visit_stack = []
dependency_cycles = []
visit = lambda do |package_id|
  return if visit_state[package_id] == :done
  if visit_state[package_id] == :visiting
    start = visit_stack.index(package_id) || 0
    dependency_cycles << (visit_stack[start..-1] + [package_id])
    return
  end

  visit_state[package_id] = :visiting
  visit_stack << package_id
  packages.fetch(package_id).fetch(:dependencies).select { |dependency| packages.key?(dependency) }.each do |dependency|
    visit.call(dependency)
  end
  visit_stack.pop
  visit_state[package_id] = :done
end
packages.each_key { |package_id| visit.call(package_id) }
dependency_cycles.uniq.each { |cycle| errors << "Work-package dependency cycle: #{cycle.join(' -> ')}" }

in_progress_tasks = leaf_tasks.select { |task| task.fetch(:state) == "~" }
if in_progress_tasks.length > 1
  errors << "Only one task may be [~]; found #{in_progress_tasks.map { |task| task.fetch(:id) }.join(', ')}"
end

# Verify local links and explicit fragments in the roadmap files.
roadmap_link_count = 0
([ROADMAP] + TASK_FILES).each do |source|
  next unless File.file?(source)

  File.read(source).scan(/\]\(([^)]+)\)/).flatten.each do |destination|
    link = destination.strip.split(/\s+['"]/, 2).first
    next if link.match?(/\A(?:https?:|mailto:)/)

    target_text, fragment = link.split("#", 2)
    target = target_text.nil? || target_text.empty? ? source : File.expand_path(target_text, File.dirname(source))
    roadmap_link_count += 1
    unless File.file?(target)
      errors << "#{relative(source)}: local link target does not exist: #{link}"
      next
    end
    next if fragment.nil? || fragment.empty?

    target_content = File.read(target)
    unless target_content.match?(/<a\s+id="#{Regexp.escape(fragment)}"\s*><\/a>/)
      errors << "#{relative(source)}: fragment ##{fragment} is not an explicit anchor in #{relative(target)}"
    end
  end
end

# Verify that the root dashboard is an exact projection of leaf tasks.
dashboard_rows = {}
roadmap.each_line do |line|
  next unless line.start_with?("| M")

  cells = line.split("|")[1...-1].map(&:strip)
  next unless cells && cells[0]&.match?(/\AM\d+\z/)

  dashboard_rows[cells[0]] = cells
end

missing_dashboard_rows = EXPECTED_MILESTONES - dashboard_rows.keys
missing_dashboard_rows.each { |milestone| errors << "TODO.md dashboard is missing #{milestone}" }
extra_dashboard_rows = dashboard_rows.keys - EXPECTED_MILESTONES
extra_dashboard_rows.each { |milestone| errors << "TODO.md dashboard has unexpected row #{milestone}" }

EXPECTED_MILESTONES.each do |milestone|
  cells = dashboard_rows[milestone]
  next unless cells

  milestone_tasks = leaf_tasks.select { |task| task.fetch(:milestone) == milestone }
  milestone_packages = packages.values.select { |package| package.fetch(:milestone) == milestone }
  milestone_priorities = Hash.new(0)
  milestone_tasks.each { |task| milestone_priorities[task.fetch(:priority)] += 1 if task.fetch(:priority) }
  expected_priority_text = "#{milestone_priorities['P0']} / #{milestone_priorities['P1']} / #{milestone_priorities['P2']}"
  expected_link = "(#{milestone_file(milestone.delete_prefix('M').to_i)}##{milestone.downcase})"

  errors << "TODO.md dashboard #{milestone} work-package count is stale" unless cells[2].to_i == milestone_packages.length
  errors << "TODO.md dashboard #{milestone} leaf-task count is stale" unless cells[3].to_i == milestone_tasks.length
  errors << "TODO.md dashboard #{milestone} priorities are stale" unless cells[4] == expected_priority_text
  errors << "TODO.md dashboard #{milestone} status is stale" unless cells[5] == derived_status(milestone_tasks)
  errors << "TODO.md dashboard #{milestone} link must use stable anchor ##{milestone.downcase}" unless cells[6]&.include?(expected_link)
end

if (summary_match = roadmap.match(/共 (\d+) 个工作包、(\d+) 个唯一叶子任务/))
  errors << "TODO.md summary work-package count is stale" unless summary_match[1].to_i == packages.length
  errors << "TODO.md summary leaf-task count is stale" unless summary_match[2].to_i == leaf_tasks.length
else
  errors << "TODO.md is missing the work-package/leaf-task summary"
end

total_line = roadmap.each_line.find { |line| line.start_with?("| **总计**") }
if total_line
  cells = total_line.split("|")[1...-1].map { |cell| cell.strip.delete("*") }
  expected_priorities = "#{priority_counts['P0']} / #{priority_counts['P1']} / #{priority_counts['P2']}"
  expected_states = "#{status_counts['x']} 完成 / #{status_counts['~']} 进行中 / #{status_counts['!']} 阻塞 / #{status_counts[' ']} 未开始"
  errors << "TODO.md total work-package count is stale" unless cells[2].to_i == packages.length
  errors << "TODO.md total leaf-task count is stale" unless cells[3].to_i == leaf_tasks.length
  errors << "TODO.md total priority counts are stale" unless cells[4] == expected_priorities
  errors << "TODO.md total task-state counts are stale" unless cells[5] == expected_states
else
  errors << "TODO.md dashboard is missing its total row"
end

next_task_match = roadmap.match(/下一任务：\[`([^`]+)`\]\(([^)]+)\)/)
if next_task_match
  next_task = leaf_tasks.find { |task| task.fetch(:id) == next_task_match[1] }
  errors << "TODO.md next task #{next_task_match[1]} does not exist" unless next_task
  errors << "TODO.md next task #{next_task_match[1]} is already completed or blocked" if next_task && %w[x !].include?(next_task.fetch(:state))
  if next_task
    expected_link = "#{relative(next_task[:file])}##{next_task[:milestone].downcase}"
    errors << "TODO.md next-task link must use #{expected_link}" unless next_task_match[2] == expected_link
  end
else
  errors << "TODO.md is missing its next-task pointer"
end

begin
  openapi = YAML.safe_load(File.read(OPENAPI_PATH), aliases: true)
  contract_operations = {}
  openapi.fetch("paths").each do |path, item|
    item.each do |method, operation|
      next unless HTTP_METHODS.include?(method) && operation.is_a?(Hash)

      operation_id = operation["operationId"]
      if operation_id.nil? || operation_id.empty?
        errors << "OpenAPI #{method.upcase} #{path} is missing operationId"
        next
      end
      if contract_operations.key?(operation_id)
        errors << "OpenAPI has duplicate operationId #{operation_id}"
      else
        contract_operations[operation_id] = {
          method: method.upcase,
          path: path,
          tag: operation.fetch("tags", []).first
        }
      end

      errors << "OpenAPI #{operation_id} is missing tags" unless operation["tags"].is_a?(Array) && !operation["tags"].empty?
      %w[security x-permission x-csrf responses].each do |field|
        errors << "OpenAPI #{operation_id} is missing #{field}" unless operation.key?(field)
      end
      errors << "OpenAPI #{operation_id} responses must be a non-empty mapping" unless operation["responses"].is_a?(Hash) && !operation["responses"].empty?
    end
  end

  errors << "Expected frozen OpenAPI baseline of 177 operations, got #{contract_operations.length}" unless contract_operations.length == 177

  coverage = JSON.parse(File.read(COVERAGE_PATH))
  errors << "Coverage schema_version must be 1" unless coverage["schema_version"] == 1
  errors << "Coverage openapi_file must be openapi/openapi.yaml" unless coverage["openapi_file"] == "openapi/openapi.yaml"
  coverage_operations = coverage.fetch("operations")
  errors << "Coverage expected_operations does not match OpenAPI" unless coverage.fetch("expected_operations") == contract_operations.length
  errors << "Coverage operation row count does not match OpenAPI" unless coverage_operations.length == contract_operations.length

  coverage_index = {}
  coverage_operations.each_with_index do |entry, index|
    missing_fields = REQUIRED_COVERAGE_FIELDS.reject { |field| entry.key?(field) }
    unless missing_fields.empty?
      errors << "#{relative(COVERAGE_PATH)} row #{index + 1} is missing fields: #{missing_fields.join(', ')}"
      next
    end

    operation_id = entry.fetch("operation_id")
    if coverage_index.key?(operation_id)
      errors << "#{relative(COVERAGE_PATH)}: duplicate operation #{operation_id}"
    else
      coverage_index[operation_id] = entry
    end

    expected = contract_operations[operation_id]
    if expected.nil?
      errors << "#{relative(COVERAGE_PATH)} row #{index + 1}: #{operation_id} is absent from OpenAPI"
      next
    end

    errors << "#{relative(COVERAGE_PATH)}: #{operation_id} method/path is stale" unless entry.fetch("method") == expected.fetch(:method) && entry.fetch("path") == expected.fetch(:path)
    errors << "#{relative(COVERAGE_PATH)}: #{operation_id} primary tag is stale" unless entry.fetch("primary_tag") == expected.fetch(:tag)

    package = packages[entry.fetch("work_package")]
    if package.nil?
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} references unknown work package #{entry.fetch('work_package')}"
    else
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} milestone disagrees with its work package" unless entry.fetch("milestone") == package.fetch(:milestone)
    end
    errors << "#{relative(COVERAGE_PATH)}: #{operation_id} has invalid priority #{entry.fetch('priority')}" unless %w[P0 P1 P2].include?(entry.fetch("priority"))
    errors << "#{relative(COVERAGE_PATH)}: #{operation_id} contract_status must be frozen" unless entry.fetch("contract_status") == "frozen"

    implementation_status = entry.fetch("implementation_status")
    errors << "#{relative(COVERAGE_PATH)}: #{operation_id} has invalid implementation status #{implementation_status}" unless ALLOWED_IMPLEMENTATION_STATES.include?(implementation_status)

    if %w[baseline_only in_progress implemented verified blocked].include?(implementation_status) && !assigned_owner?(entry.fetch("owner"))
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} status #{implementation_status} requires an assigned owner"
    end
    if %w[baseline_only implemented verified].include?(implementation_status)
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} status #{implementation_status} requires a handler" unless entry["handler"].is_a?(String) && !entry["handler"].empty?
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} status #{implementation_status} requires tests" unless entry["tests"].is_a?(Array) && !entry["tests"].empty?
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} status #{implementation_status} requires evidence" unless entry["evidence"].is_a?(String) && !entry["evidence"].empty?
    end
    if implementation_status == "blocked"
      evidence = entry["evidence"].to_s
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} blocked status requires review metadata" unless evidence.include?("复查") && evidence.include?("解除")
    end

    if package && %w[in_progress implemented verified blocked].include?(implementation_status) && derived_status(package.fetch(:tasks)) == "未开始"
      errors << "#{relative(COVERAGE_PATH)}: #{operation_id} is #{implementation_status} while #{package.fetch(:id)} has no active/completed/blocked leaf task"
    end
  end

  contract_operations.each_key do |operation_id|
    errors << "#{relative(COVERAGE_PATH)}: missing OpenAPI operation #{operation_id}" unless coverage_index.key?(operation_id)
  end
rescue StandardError => error
  errors << "Could not validate OpenAPI coverage: #{error.class}: #{error.message}"
end

if errors.empty?
  puts "Roadmap OK: #{leaf_tasks.length} unique leaf tasks across #{packages.length} work packages"
  puts "Task states: pending=#{status_counts[' ']}, in_progress=#{status_counts['~']}, completed=#{status_counts['x']}, blocked=#{status_counts['!']}"
  puts "Priorities: P0=#{priority_counts['P0']}, P1=#{priority_counts['P1']}, P2=#{priority_counts['P2']}"
  puts "Dependencies: valid and acyclic across #{packages.length} work packages"
  puts "Dashboard: M0-M17 counts, states, links and anchors are current"
  puts "Local roadmap links: #{roadmap_link_count} checked"
  puts "OpenAPI coverage: 177/177 operations assigned"
else
  warn "Roadmap validation failed with #{errors.length} error(s):"
  errors.each { |error| warn "- #{error}" }
  exit 1
end
