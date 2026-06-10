//! `user register | login | whoami`.

use std::io::Read;

use serde::Deserialize;
use serde_json::json;

use crate::cli::client::Client;
use crate::cli::output::{print_json, print_line, CliError, CliResult};
use crate::cli::UserCmd;

#[derive(Debug, Deserialize)]
struct AuthResponse {
    user_id: String,
    username: String,
    api_key: String,
    #[serde(default)]
    key_id: Option<String>,
}

pub async fn run(client: Client, json: bool, cmd: UserCmd) -> CliResult<()> {
    match cmd {
        UserCmd::Register {
            username,
            password,
            password_stdin,
        } => {
            let password = resolve_password(password, password_stdin)?;
            let resp: AuthResponse = client
                .post_noauth(
                    "/api/v1/auth/register",
                    &json!({ "username": username, "password": password }),
                )
                .await?;
            emit_auth(json, &resp)
        }
        UserCmd::Login {
            username,
            password,
            password_stdin,
        } => {
            let password = resolve_password(password, password_stdin)?;
            let resp: AuthResponse = client
                .post_noauth(
                    "/api/v1/auth/login",
                    &json!({ "username": username, "password": password }),
                )
                .await?;
            emit_auth(json, &resp)
        }
        UserCmd::Whoami => {
            let v: serde_json::Value = client.get("/api/v1/auth/whoami").await?;
            if json {
                print_json(&v)
            } else {
                let username = v.get("username").and_then(|x| x.as_str()).unwrap_or("?");
                let user_id = v.get("user_id").and_then(|x| x.as_str()).unwrap_or("?");
                print_line(format!("{username}  ({user_id})"));
                Ok(())
            }
        }
    }
}

fn emit_auth(json: bool, resp: &AuthResponse) -> CliResult<()> {
    if json {
        return print_json(&json!({
            "user_id": resp.user_id,
            "username": resp.username,
            "api_key": resp.api_key,
            "key_id": resp.key_id,
        }));
    }
    print_line(format!("user_id: {}", resp.user_id));
    print_line(format!("username: {}", resp.username));
    if let Some(id) = &resp.key_id {
        print_line(format!("key_id: {id}"));
    }
    print_line(format!("api_key: {}", resp.api_key));
    print_line("");
    print_line("Save the api_key now — it is not recoverable.");
    Ok(())
}

/// Resolve a password from `--password`, `--password-stdin`, or fail.
fn resolve_password(flag: Option<String>, from_stdin: bool) -> CliResult<String> {
    if from_stdin {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        return Ok(buf.trim_end_matches(['\n', '\r']).to_string());
    }
    if let Some(p) = flag {
        return Ok(p);
    }
    Err(CliError::Usage(
        "password required: pass --password <pw> or --password-stdin".into(),
    ))
}
