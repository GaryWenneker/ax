//! Local client setup, independent from a configured remote publisher.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LocalClient {
    Cursor,
    Vscode,
    Cline,
    ClaudeCode,
    Other,
}
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Target {
    ThisComputer,
    ProjectFolder,
}

pub(crate) fn generate(
    root: &str,
    executable: &str,
    workspace_id: &str,
    client: LocalClient,
    target: Target,
) -> Result<Value, &'static str> {
    if target == Target::ProjectFolder
        && !matches!(client, LocalClient::Cursor | LocalClient::Vscode)
    {
        return Err("Share a project-folder configuration with Cursor or VS Code, or use a remote connection.");
    }
    let portable = target == Target::ProjectFolder;
    let command = if portable { "ax" } else { executable };
    let project = if portable { "${workspaceFolder}" } else { root };
    let mut server = json!({"command":command,"args":["serve","--mcp","--path",project]});
    match client {
        LocalClient::Vscode => {
            server["type"] = json!("stdio");
            server["cwd"] = json!(project);
        }
        LocalClient::ClaudeCode => {
            server["type"] = json!("stdio");
        }
        LocalClient::Cline => {
            server["disabled"] = json!(false);
            server["autoApprove"] = json!([]);
        }
        _ => {}
    }
    let (destination, file_name) = match client {
        LocalClient::Cursor => (".cursor/mcp.json", "mcp.json"),
        LocalClient::Vscode => (".vscode/mcp.json", "mcp.json"),
        LocalClient::ClaudeCode => (".mcp.json", ".mcp.json"),
        LocalClient::Cline => (
            "Cline → MCP Servers → Configure MCP Servers",
            "cline_mcp_settings.json",
        ),
        LocalClient::Other => ("Your app's MCP settings", "mcp.json"),
    };
    let config = if client == LocalClient::Vscode {
        json!({"servers":{"ax":server}})
    } else {
        json!({"mcpServers":{"ax":server}})
    };
    let content =
        serde_json::to_string_pretty(&config).map_err(|_| "Cannot generate configuration")?;
    let instructions = if portable {
        vec![format!("Add the ax server entry to {destination} in the project and share that file with the project."), "Each person installs Ax, opens their own copy of the project, runs ax init there and enables the MCP server in their app. The ax command must be on that computer's PATH.".into(), "This shares the setup for each person's local project. To access the project hosted on this computer from elsewhere, use the remote sharing card below.".into()]
    } else {
        vec![format!("Open {destination} and merge the ax server entry. Keep any other servers already in that file."), "Reload your app or restart its Ax MCP server. This configuration uses Ax and the active project on this computer; no remote server or OAuth setup is needed.".into(), "The file includes this computer's local paths. Use Share with project or a remote connection when sending setup to another computer.".into()]
    };
    Ok(
        json!({"workspaceId":workspace_id,"client":client,"target":target,"format":"json","fileName":file_name,"destination":destination,"content":content,"instructions":instructions,"containsSecrets":false,"containsLocalPaths":!portable,"portable":portable}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_configs_pin_the_active_project_and_use_client_specific_schema() {
        let root = r#"C:\Projects\my "project""#;
        for client in [
            LocalClient::Cursor,
            LocalClient::Vscode,
            LocalClient::Cline,
            LocalClient::ClaudeCode,
            LocalClient::Other,
        ] {
            let result = generate(
                root,
                r#"C:\Ax Tools\ax.exe"#,
                "workspace",
                client,
                Target::ThisComputer,
            )
            .unwrap();
            let value: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
            let server = &value[if client == LocalClient::Vscode {
                "servers"
            } else {
                "mcpServers"
            }]["ax"];
            assert_eq!(server["args"], json!(["serve", "--mcp", "--path", root]));
            assert_eq!(server["command"], r#"C:\Ax Tools\ax.exe"#);
            assert_eq!(result["containsLocalPaths"], true);
            assert_eq!(result["containsSecrets"], false);
            assert!(server.get("env").is_none());
            assert!(server.get("url").is_none());
            if client == LocalClient::Vscode || client == LocalClient::ClaudeCode {
                assert_eq!(server["type"], "stdio");
            }
            if client == LocalClient::Cline {
                assert_eq!(server["autoApprove"], json!([]));
            }
        }
    }
    #[test]
    fn shareable_setup_uses_verified_project_variables_and_excludes_host_paths() {
        for client in [LocalClient::Cursor, LocalClient::Vscode] {
            let result = generate(
                "/private/host/project",
                "/private/host/bin/ax",
                "workspace",
                client,
                Target::ProjectFolder,
            )
            .unwrap();
            assert!(!result["content"].as_str().unwrap().contains("/private"));
            assert!(result["content"]
                .as_str()
                .unwrap()
                .contains("${workspaceFolder}"));
            assert_eq!(result["portable"], true);
            assert_eq!(result["containsLocalPaths"], false);
        }
        assert!(generate("/p", "ax", "w", LocalClient::Cline, Target::ProjectFolder).is_err());
    }
}
