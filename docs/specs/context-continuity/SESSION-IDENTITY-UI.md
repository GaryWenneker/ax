# Session identity

The Logging page shows **Test context continuity**.

The fields are:

- External Agent Chat ID
- External Agent Turn ID
- Ax Session ID
- Ax Turn ID
- Agent provider
- Project identifier
- Session creation time
- Parent session reference

Copy actions:

- Copy Chat ID
- Copy Turn ID
- Copy Ax Session ID
- Copy All IDs
- Copy Context Test Reference

A blank provider field is `not issued`. Copying that field returns `external chat id was not issued` or `external turn id was not issued`. The error does not contain a generated id. Copy All lists `not issued` for the blank fields and the real Ax session id for the field that has one.

The project id is the open project. The panel does not invent a provider, a chat id, or a turn id from it.
