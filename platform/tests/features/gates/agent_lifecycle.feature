@gate @ops @wip
Feature: LaunchAgent lifecycle management
  Jeff runs agent-state.sh to see which services are up, restart crashed ones,
  and find orphan processes. No more guessing from raw launchctl output.

  Background:
    Given agent-state.sh exists in platform/scripts

  Scenario: Health summary shows running, crashed, and dead counts
    When Jeff runs "agent-state.sh health"
    Then he sees total agent count
    And he sees how many are running, crashed, and stopped
    And he sees critical service status with green/red indicators

  Scenario: Status lists all agents with PID and state
    When Jeff runs "agent-state.sh status"
    Then he sees a table with AGENT, PID, EXIT, and STATE columns
    And each agent shows RUNNING, CRASHED, or DEAD
    And both com.chorus and com.gathering agents appear

  Scenario: Status filter narrows to matching agents
    When Jeff runs "agent-state.sh status api"
    Then he sees only agents matching "api"
    And unrelated agents like fuseki do not appear

  Scenario: Short name resolves to full label
    When Jeff runs "agent-state.sh start heartbeat"
    Then agent-state resolves "heartbeat" to "com.chorus.heartbeat"
    And launchctl kickstart fires for that label

  Scenario: Orphan scan finds processes with ppid=1 on known ports
    When Jeff runs "agent-state.sh orphans"
    Then the script checks all known service ports
    And orphan processes (ppid=1) are listed with port and command
    And Jeff is prompted before any kill

  Scenario: Nonexistent service name errors clearly
    When Jeff runs "agent-state.sh start fake-service-xyz"
    Then he sees "No agent found matching 'fake-service-xyz'"

  Scenario: No args shows usage
    When Jeff runs "agent-state.sh" with no arguments
    Then he sees usage instructions with all available commands
