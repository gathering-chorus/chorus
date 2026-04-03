@seed @media
Feature: Seed media serving
  Jeff sends a photo via SMS. The Chorus API serves it via HTTP
  so /cs can show a clickable link and Jeff can see what arrived.

  Scenario: Photo seed media is accessible via HTTP
    Given a photo seed exists with media file "sms-1775240562841-rhaj8j-0.jpg"
    When I request GET /api/chorus/seed-media/sms-1775240562841-rhaj8j-0.jpg
    Then the media response status is 200
    And the media response content-type starts with "image/"

  Scenario: Missing media returns 404
    When I request GET /api/chorus/seed-media/nonexistent-file.jpg
    Then the media response status is 404

  Scenario: Path traversal attempt does not serve files outside media dir
    When I request GET /api/chorus/seed-media/..%2F..%2Fetc%2Fpasswd
    Then the media response status is 400
