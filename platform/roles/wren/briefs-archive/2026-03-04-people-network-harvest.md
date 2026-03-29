# People & Network Graph — New Collection

**From:** Kade | **To:** Wren | **Priority:** scope card needed

## What Jeff Wants

A people/network view — who he's connected to across social platforms, visualized as a relationship graph. Came up naturally after the social posts collection (#443) and the DSL conversation.

## Data Available

**Facebook** (in Downloads, raw export):
- `connections/friends/your_friends.json` — **334 friends** with names + timestamps
- `connections/friends/sent_friend_requests.json`, `received_friend_requests.json`
- `personal_information/other_personal_information/your_imported_contacts.json` — phone contacts synced to FB
- `personal_information/profile_information/contacts_uploaded_before_2021.json`
- `logged_information/activity_messages/people_and_friends.json`

**LinkedIn**:
- Current export only had Shares.csv (posts). **Connections.csv not present** — Jeff may need to re-download with the connections data included.
- LinkedIn exports include: first name, last name, company, position, connected date.

**Already harvested posts** (2,075 in RDF):
- Posts may contain tagged people, mentions, comments — not yet extracted as entities.

## Scope Questions for Wren

1. **Just contacts/connections first?** Or full relationship graph (who connects to who)?
2. **People as a new collection** (`/collection/people`) or a layer on social posts?
3. **Cross-reference** — same person on FB + LinkedIn? Name matching is fuzzy.
4. **Privacy** — this is the most personally sensitive data we've harvested. Visibility controls matter.
5. **LinkedIn re-export** — Jeff needs to request a new download that includes Connections.csv.

## Engineering Notes

- Harvest pattern exists from #443 — CSV/JSON → TTL → pod service → search index
- `jb:Person` type would be new in the ontology
- Network visualization could reuse the mind map's D3 force graph
- 334 FB friends + ~500-1000 LinkedIn connections = manageable dataset
