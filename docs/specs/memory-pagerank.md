# Personalized PageRank activation

Status: Phase 6 graph-activation slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. With edges a—b—c and seed `a`, after enough iterations `rank(a) > rank(c)`.

## Known limits

- Wiring PPR scores into the live recall fusion sum remains a follow-up.
