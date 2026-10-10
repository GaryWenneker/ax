# Remote MCP validation map

The implementation is integrated on main `3fe6546` (PR #10). This feature adds the remote adapter without replacing local stdio or the Smart Output dispatcher.

| Boundary | Executable evidence |
|---|---|
| JWT signature, issuer, audience, expiry, nbf and scope | `remote_tokens_require_signature_issuer_audience_expiry_and_scope` |
| Auth before project/engine access | `remote_authentication_precedes_project_and_engine_access` |
| Principal, project and transport lifecycle | `remote_transport_enforces_session_project_principal_and_lifecycle` |
| Discovery and direct tool permissions, scoped write challenge | `remote_tools_deny_execution_writes_root_overrides_and_foreign_chats`, `remote_discovery_advertises_oauth_and_rejects_invalid_identity_without_borrowing` |
| Explicit write grant and global-policy denial | `remote_writes_require_scope_and_the_explicit_project_grant`, `remote_authorized_writer_can_remember_but_cannot_save_global_policy` |
| Chat/cache ownership and scoped branches | `remote_chats_and_caches_are_connection_owned`, `remote_context_reset_rehydrates_policy_and_fork_keeps_connection_ownership`, `remote_durable_branches_register_only_server_issued_child_ids` |
| Real HTTP wire and ephemeral port | `remote_http_wire_negotiates_initializes_lists_and_rejects_a_bad_version`, `remote_dynamic_port_does_not_replace_an_occupied_listener` |
| Unix upstream | `remote_socket_refuses_existing_paths_without_removing_them`; `remote_socket_forwards_to_actual_dynamic_port` is enforced by `AX_TEST_UNIX_SOCKETS=1` in macOS/Linux CI |
| Native OAuth | Callback unit tests reject foreign Host/state/issuer, duplicate code and denial; cancellation test completes a pending attempt; local provider test verifies code verifier, resource, redirect, refresh rotation and project-access denial |
| Secure credentials | Injected OS-store failure cannot fall back to plaintext; mock store deletion removes the secret; metadata contains no access/refresh token |
| Local Settings controls | `local_login_controls_reject_foreign_origin_and_csrf` |
| Browser UI | `e2e/remote-mcp.spec.ts`: connect/fallback/poll/disconnect, denial and pending cancellation with CSRF |

Local validation uses workspace build, workspace Clippy with warnings denied, workspace tests, `scripts/check-home-dir.sh`, the actual embedded UI build, three focused Playwright browser tests, and the complete documentation build plus 26 documentation tests. GitHub CI repeats workspace builds/tests on Windows, macOS and Ubuntu, Clippy, browser UI and the docs build before PR creation. The CI result and exact head SHA belong in the PR description; this document does not stand in for those results.

The managed local environment forbids Unix sockets, so the real Unix bridge check runs in macOS/Linux CI. Native credential behavior is tested with a mock OS credential rather than the owner's logged-in keychain.

## Deployment checks still requiring the owner's configuration

The mock provider does not prove a real provider, Cloudflare route or ChatGPT connection. The owner must supply the issuer/JWKS, real project roots and subject grants, public native client registration and separate ChatGPT registration. Register the actual callback shown by ChatGPT. Keep the Mac awake and tunnel running. No provider configuration, tunnel rewrite or installation on the owner's Mac is claimed.

Website publication is separate from the Remote service. The docs workflow validates feature branches and publishes main only after that commit's workspace CI succeeds, using the existing repository Netlify secrets. Missing deployment secrets or a failed deploy fail visibly. Validate the live `/guides/remote-mcp/` page and Netlify deploy after merge; a built artifact alone is not live publication.
