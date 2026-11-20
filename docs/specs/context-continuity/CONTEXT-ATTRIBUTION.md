# Context attribution

Each item delivered to Arm B records:

- Source, `Kind:id`
- Original session id
- Original turn id, when the item has one
- Context version
- Retrieval method `tag-overlap`
- Relevance score, matching tags divided by the task tags
- Delivery timestamp
- `received`, true when the item is in the bundle given to the agent
- `use_observed`, true only when the fixture agent cited the id

Retrieval is not use. The project objective is delivered with the unfinished-feature task and is not cited, so `received` is true and `use_observed` is false. The pending-feature item is both.

Diagnostic mode fills `why` with the overlap count. With diagnostic mode off, `why` is empty.
