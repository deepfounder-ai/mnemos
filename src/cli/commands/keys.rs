//! `keys list | create | revoke`.

use serde::Deserialize;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::output::{print_json, print_line, CliResult};
use crate::cli::KeysCmd;

#[derive(Debug, Deserialize)]
struct KeyView {
    id: String,
    name: String,
    created_at: Option<String>,
    last_used_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KeyList {
    keys: Vec<KeyView>,
}

pub async fn run(client: Client, json: bool, cmd: KeysCmd) -> CliResult<()> {
    match cmd {
        KeysCmd::List => {
            let list: KeyList = client.get("/api/v1/auth/keys").await?;
            if json {
                return print_json(&json!({ "keys": list.keys.iter().map(|k| json!({
                    "id": k.id, "name": k.name, "created_at": k.created_at, "last_used_at": k.last_used_at
                })).collect::<Vec<_>>() }));
            }
            print_line(format!(
                "{:<38} {:<24} {:<22} {}",
                "ID", "NAME", "CREATED", "LAST USED"
            ));
            for k in &list.keys {
                print_line(format!(
                    "{:<38} {:<24} {:<22} {}",
                    k.id,
                    k.name,
                    k.created_at.as_deref().unwrap_or("-"),
                    k.last_used_at.as_deref().unwrap_or("never"),
                ));
            }
            Ok(())
        }
        KeysCmd::Create { name } => {
            let v: serde_json::Value = client
                .post("/api/v1/auth/keys", &json!({ "name": name }))
                .await?;
            if json {
                return print_json(&v);
            }
            let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("?");
            let key = v.get("api_key").and_then(|x| x.as_str()).unwrap_or("?");
            print_line(format!("key_id: {id}"));
            print_line(format!("api_key: {key}"));
            print_line("");
            print_line("Save the api_key now — it is shown only once.");
            Ok(())
        }
        KeysCmd::Revoke { id } => {
            let _: serde_json::Value = client.delete(&format!("/api/v1/auth/keys/{id}")).await?;
            if json {
                return print_json(&json!({ "revoked": id }));
            }
            print_line(format!("revoked {id}"));
            Ok(())
        }
    }
}
