//! Pure client configuration generation. No credentials, grants or process startup.
use ax_global_db::remote_mcp_publications::Publication;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Client {
    Cursor,
    Vscode,
    Cline,
    ClaudeCode,
    Chatgpt,
    AxProxy,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Direct,
    Proxy,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub workspace_id: String,
    pub publication_id: String,
    pub config_fingerprint: String,
    pub client: Client,
    pub server_key: String,
    pub format: &'static str,
    pub file_name: Option<&'static str>,
    pub content: String,
    pub instructions: Vec<String>,
    pub requirements: Vec<String>,
    pub login_command: Option<String>,
    pub contains_secrets: bool,
}
/// Stable, alphanumeric keys, distinct for publishers with the same project ID.
pub fn server_key(p: &Publication) -> String {
    let project: String = p
        .remote_project_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    format!(
        "axRemote{project}{}{}",
        &p.publisher_id[..p.publisher_id.len().min(8)],
        &p.id[..p.id.len().min(8)]
    )
}
pub fn generate(
    p: &Publication,
    workspace_id: &str,
    client: Client,
    transport: Transport,
) -> Result<Export, String> {
    let endpoint = super::config::https_url(&p.endpoint_url)?;
    let origin = super::config::https_url(&p.public_origin)?;
    if endpoint.origin() != origin.origin()
        || endpoint.path() != format!("/projects/{}/mcp", p.remote_project_id)
    {
        return Err("Publication endpoint does not match its registered project".into());
    }
    if matches!(client, Client::Chatgpt | Client::ClaudeCode) && transport == Transport::Proxy {
        return Err("This preset supports direct HTTP only".into());
    }
    if client == Client::AxProxy && transport != Transport::Proxy {
        return Err("Ax proxy requires stdio transport".into());
    }
    let clients = p
        .clients()
        .map_err(|_| "Invalid public client registration")?;
    let key = server_key(p);
    let mut requirements = Vec::new();
    let mut instructions = Vec::new();
    let mut login_command = None;
    let (format, file_name, content) = if client == Client::Chatgpt {
        instructions.push("In ChatGPT, enable developer mode if available, then create a custom app with this MCP URL and OAuth. Availability depends on your account and workspace. Register ChatGPT separately with your OAuth provider.".into());
        ("url", None, p.endpoint_url.clone())
    } else {
        let mut server = if transport == Transport::Proxy {
            requirements.push("Install Ax on the computer that runs your MCP client. Sign in there before starting the proxy.".into());
            if let Some(id) = &clients.ax_proxy {
                // POSIX single-quoted arguments prevent shell expansion of public IDs.
                let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
                login_command = Some(format!(
                    "ax mcp remote login --url {} --client-id {}",
                    quote(&p.endpoint_url),
                    quote(id)
                ));
                instructions.push("Run the displayed login command in a POSIX shell on the receiving computer. In PowerShell, use the same arguments and double any single quotes inside the public client ID.".into());
            } else {
                requirements.push("Register a native Ax OAuth client and add its public ID to this publication to generate the login command. A native Ax ID is separate from an IDE's OAuth registration.".into());
            }
            let mut v =
                json!({"command":"ax","args":["mcp","remote","proxy","--url",p.endpoint_url]});
            if client == Client::Vscode {
                v["type"] = json!("stdio");
            }
            v
        } else {
            let mut v = json!({"url":p.endpoint_url});
            match client {
                Client::Cursor => {
                    if let Some(id) = &clients.cursor {
                        v["auth"] = json!({"CLIENT_ID":id,"scopes":["ax:context"]});
                    } else {
                        requirements.push("OAuth registration for Cursor is unconfirmed. Configure a public Cursor client with your provider, or use Ax proxy.".into());
                    }
                }
                Client::Vscode => {
                    v["type"] = json!("http");
                    if let Some(id) = &clients.vscode {
                        v["oauth"] = json!({"clientId":id});
                    } else {
                        requirements.push("OAuth registration for VS Code is unconfirmed. Configure a public VS Code client with your provider, or use Ax proxy.".into());
                    }
                }
                Client::ClaudeCode => {
                    v["type"] = json!("http");
                    requirements.push("Authorize Claude Code separately with your OAuth provider. This preset contains no static credential.".into());
                }
                Client::Cline => {
                    v["type"] = json!("streamableHttp");
                    requirements.push("Use direct OAuth only with a Cline surface/version and provider you have verified. Ax proxy is the default fallback.".into());
                }
                _ => return Err("Unsupported direct client".into()),
            }
            v
        };
        if client == Client::Cline {
            server["disabled"] = json!(false);
            server["autoApprove"] = json!([]);
        }
        let (filename, document): (&str, Value) = match client {
            Client::Vscode => ("mcp.json", json!({"servers":{key.clone():server}})),
            Client::Cursor => ("mcp.json", json!({"mcpServers":{key.clone():server}})),
            Client::ClaudeCode => (".mcp.json", json!({"mcpServers":{key.clone():server}})),
            Client::Cline => (
                "cline_mcp_settings.json",
                json!({"mcpServers":{key.clone():server}}),
            ),
            _ => ("mcp.json", json!({"mcpServers":{key.clone():server}})),
        };
        instructions.push(match client {
            Client::Vscode => "Merge this server entry into .vscode/mcp.json in the receiving project.",
            Client::Cursor => "Merge this server entry into .cursor/mcp.json in the receiving project.",
            Client::ClaudeCode => "Merge this server entry into the receiving project's .mcp.json, then authorize the server in Claude Code.",
            Client::Cline => "Merge this server entry into Cline's MCP settings file.",
            _ => "Merge this server entry into your client's MCP configuration.",
        }.into());
        instructions.push("Preserve other servers in the destination file. Each receiving user must already have access to this remote project; copying does not grant access.".into());
        (
            "json",
            Some(filename),
            serde_json::to_string_pretty(&document).map_err(|e| e.to_string())?,
        )
    };
    Ok(Export {
        workspace_id: workspace_id.into(),
        publication_id: p.id.clone(),
        config_fingerprint: p.config_fingerprint.clone(),
        client,
        server_key: key,
        format,
        file_name,
        content,
        instructions,
        requirements,
        login_command,
        contains_secrets: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn publication() -> Publication {
        Publication {
            id: "11111111".repeat(8),
            publisher_id: "22222222".repeat(8),
            public_origin: "https://ax.example".into(),
            oauth_issuer: "https://issuer.example/".into(),
            resource: "https://ax.example".into(),
            publisher_label: "Server".into(),
            config_fingerprint: "version".into(),
            remote_project_id: "remote-demo".into(),
            project_label: "demo".into(),
            endpoint_url: "https://ax.example/projects/remote-demo/mcp".into(),
            canonical_project_root: "/private/project".into(),
            client_registrations:
                r#"{"axProxy":"native-public","cursor":"cursor-public","vscode":"vscode-public"}"#
                    .into(),
            runtime_status: "stopped".into(),
            last_heartbeat_at: None,
        }
    }
    #[test]
    fn client_presets_use_distinct_schema_and_only_their_own_public_oauth_ids() {
        let p = publication();
        for client in [
            Client::Cursor,
            Client::Vscode,
            Client::ClaudeCode,
            Client::Cline,
        ] {
            let result = generate(&p, "workspace", client, Transport::Direct).unwrap();
            let value: Value = serde_json::from_str(&result.content).unwrap();
            let server = &value[if client == Client::Vscode {
                "servers"
            } else {
                "mcpServers"
            }][&result.server_key];
            assert_eq!(server["url"], p.endpoint_url);
            match client {
                Client::Cursor => {
                    assert_eq!(server["auth"]["CLIENT_ID"], "cursor-public");
                    assert!(server.get("type").is_none());
                }
                Client::Vscode => {
                    assert!(value.get("mcpServers").is_none());
                    assert_eq!(server["type"], "http");
                    assert_eq!(server["oauth"]["clientId"], "vscode-public");
                }
                Client::ClaudeCode => {
                    assert_eq!(server["type"], "http");
                    assert!(server.get("oauth").is_none());
                }
                Client::Cline => {
                    assert_eq!(server["type"], "streamableHttp");
                    assert_eq!(server["autoApprove"], json!([]));
                    assert_eq!(server["disabled"], false);
                }
                _ => unreachable!(),
            }
            assert!(!result.content.contains("native-public"));
            assert!(!result.content.contains("/private"));
            assert!(!result.content.contains("secret"));
            assert!(!result.contains_secrets);
        }
    }
    #[test]
    fn proxy_args_are_cross_platform_and_missing_registration_is_explicit() {
        let mut p = publication();
        p.client_registrations = "{}".into();
        for client in [
            Client::Cursor,
            Client::Vscode,
            Client::Cline,
            Client::AxProxy,
        ] {
            let result = generate(&p, "workspace", client, Transport::Proxy).unwrap();
            let value: Value = serde_json::from_str(&result.content).unwrap();
            let server = &value[if client == Client::Vscode {
                "servers"
            } else {
                "mcpServers"
            }][&result.server_key];
            assert_eq!(
                server["args"],
                json!(["mcp", "remote", "proxy", "--url", p.endpoint_url])
            );
            assert!(result.login_command.is_none());
            assert!(result.requirements.iter().any(|v| v.contains("native Ax")));
            if client != Client::Cline {
                assert!(server.get("autoApprove").is_none());
            }
        }
        let chatgpt = generate(&p, "workspace", Client::Chatgpt, Transport::Direct).unwrap();
        assert_eq!(chatgpt.format, "url");
        assert_eq!(chatgpt.content, p.endpoint_url);
        assert!(chatgpt.file_name.is_none());
        assert!(generate(&p, "workspace", Client::Chatgpt, Transport::Proxy).is_err());
    }
    #[test]
    fn similar_project_ids_and_different_hosts_do_not_collide() {
        let p = publication();
        let mut another = p.clone();
        another.publisher_id = "33333333".repeat(8);
        assert_ne!(server_key(&p), server_key(&another));
        another = p.clone();
        another.remote_project_id = "remotedemo".into();
        another.id = "44444444".repeat(8);
        assert_ne!(server_key(&p), server_key(&another));
        another.endpoint_url = "https://attacker.example/projects/remote-demo/mcp".into();
        assert!(generate(&another, "workspace", Client::Cursor, Transport::Direct).is_err());
    }
}
