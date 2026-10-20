// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! `tess-transfer`: export and import Tess content models and users, as YAML
//! or JSON, through the GraphQL API (so it works with every deployment and
//! database).
//!
//! ```text
//! export TESS_URL=http://localhost:8080
//! export TESS_TOKEN=…                         # Admin session, API token or bootstrap token
//! tess-transfer export -o tess.yaml            # models + users
//! tess-transfer export --only models -o models.json
//! tess-transfer import tess.yaml --dry-run
//! tess-transfer import tess.yaml --passwords-out new-users.txt
//! tess-transfer import models.yaml --update
//! ```

mod api;
mod document;

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use crate::api::Client;
use crate::document::{ComponentDoc, Document, FORMAT_VERSION, ModelDef, UserDef, component_refs, relation_targets};

/// Content types managed by Tess itself.
const BUILTIN_TYPES: &[&str] = &["Asset"];

#[derive(Parser)]
#[command(
    version,
    about = "Export or import Tess content models and users (YAML or JSON).",
    after_help = "The token needs the Admin role (Owner to import Owners). It is read from \
                  TESS_TOKEN only, so it never appears in your shell history.\n\
                  Exports never contain passwords. Files with users contain staff email \
                  addresses: don't commit them."
)]
struct Cli {
    /// Base URL of the Tess server.
    #[arg(long, env = "TESS_URL", default_value = "http://localhost:8080", global = true)]
    url: String,
    /// File format (default: from the file extension, else YAML).
    #[arg(long, value_enum, global = true)]
    format: Option<Format>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write content models and/or users to a file (or stdout).
    Export {
        /// Output file; stdout when omitted or `-`.
        #[arg(short, long)]
        output: Option<String>,
        #[arg(long, value_enum)]
        only: Option<Only>,
    },
    /// Create content models and/or users from a file (`-` for stdin).
    Import {
        file: String,
        #[arg(long, value_enum)]
        only: Option<Only>,
        /// Replace the definition of existing models and update existing users.
        #[arg(long)]
        update: bool,
        /// Show what would change without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// File receiving the random passwords of new users (created with mode 600).
        #[arg(long)]
        passwords_out: Option<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Yaml,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Only {
    Models,
    Users,
}

fn format_for(path: Option<&str>, explicit: Option<Format>) -> Format {
    explicit.unwrap_or(match path {
        Some(p) if p.to_ascii_lowercase().ends_with(".json") => Format::Json,
        _ => Format::Yaml,
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli).await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let token = std::env::var("TESS_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .context("set TESS_TOKEN (an Admin session token, API token or the bootstrap token)")?;
    let client = Client::new(&cli.url, token)?;
    match cli.command {
        Command::Export { output, only } => export(&client, output.as_deref(), only, cli.format).await,
        Command::Import { file, only, update, dry_run, passwords_out } => {
            let options = ImportOptions { update, dry_run, passwords_out };
            import(&client, &file, only, cli.format, &options).await
        }
    }
}

// ---- export -------------------------------------------------------------------

async fn export(client: &Client, output: Option<&str>, only: Option<Only>, format: Option<Format>) -> Result<()> {
    let mut doc = Document { version: FORMAT_VERSION, components: None, content_types: None, users: None };
    if only != Some(Only::Users) {
        let mut components = client.components().await?;
        components.sort_by(|a, b| a.api_id.cmp(&b.api_id));
        doc.components = Some(components);
        let mut models: Vec<ModelDef> = client
            .models()
            .await?
            .into_iter()
            .filter(|m| !BUILTIN_TYPES.contains(&m.api_id.as_str()))
            .collect();
        models.sort_by(|a, b| a.api_id.cmp(&b.api_id));
        doc.content_types = Some(models);
    }
    if only != Some(Only::Models) {
        let mut users = client.users().await?;
        users.sort_by(|a, b| a.def.email.cmp(&b.def.email));
        doc.users = Some(users.into_iter().map(|u| u.def).collect());
    }
    let to_file = output.filter(|o| *o != "-");
    let text = doc.render(format_for(to_file, format) == Format::Json)?;
    match to_file {
        Some(path) => {
            std::fs::write(path, text).with_context(|| format!("cannot write {path}"))?;
            let mut parts = vec![];
            if let Some(c) = &doc.components {
                parts.push(format!("{} components", c.len()));
            }
            if let Some(m) = &doc.content_types {
                parts.push(format!("{} content types", m.len()));
            }
            if let Some(u) = &doc.users {
                parts.push(format!("{} users", u.len()));
            }
            eprintln!("exported {} to {path}", parts.join(", "));
            if doc.users.is_some() {
                eprintln!("note: this file contains staff email addresses; do not commit it");
            }
        }
        None => std::io::stdout().write_all(text.as_bytes())?,
    }
    Ok(())
}

// ---- import -------------------------------------------------------------------

struct ImportOptions {
    update: bool,
    dry_run: bool,
    passwords_out: Option<String>,
}

/// What an import will do, computed and validated before any change.
#[derive(Default)]
struct Plan {
    /// New components in dependency order, with relation targets deferred to pass 2.
    create_components: Vec<(ComponentDoc, BTreeSet<String>)>,
    /// Full component definitions applied in pass 2.
    update_components: Vec<(ComponentDoc, &'static str)>,
    skipped_components: Vec<String>,
    /// New models in creation order, with the relation targets deferred to pass 2.
    create: Vec<(ModelDef, BTreeSet<String>)>,
    /// Full definitions applied in pass 2: "complete" (deferred relations) or "update".
    update: Vec<(ModelDef, &'static str)>,
    skipped_models: Vec<String>,
    create_users: Vec<UserDef>,
    update_users: Vec<(String, UserDef)>,
    skipped_users: Vec<String>,
}

async fn import(
    client: &Client,
    file: &str,
    only: Option<Only>,
    format: Option<Format>,
    options: &ImportOptions,
) -> Result<()> {
    let mut text = String::new();
    if file == "-" {
        std::io::stdin().read_to_string(&mut text)?;
    } else {
        text = std::fs::read_to_string(file).with_context(|| format!("cannot read {file}"))?;
    }
    let doc = Document::parse(&text, format_for(Some(file), format) == Format::Json)?;
    let models = if only == Some(Only::Users) { vec![] } else { doc.content_types.unwrap_or_default() };
    let components = if only == Some(Only::Users) { vec![] } else { doc.components.unwrap_or_default() };
    let users = if only == Some(Only::Models) { vec![] } else { doc.users.unwrap_or_default() };

    // Everything is validated before the first change.
    let plan = plan(client, components, models, users, options).await?;
    let dry = if options.dry_run { " [dry run]" } else { "" };
    let mut failures = 0;

    if !plan.create_components.is_empty() || !plan.update_components.is_empty() || !plan.skipped_components.is_empty() {
        eprintln!("components{dry}:");
    }
    for reason in &plan.skipped_components {
        eprintln!("  skip {reason}");
    }
    for (c, deferred) in &plan.create_components {
        let note = if deferred.is_empty() { "" } else { " (relations added in a second pass)" };
        eprintln!("  create {}{note}", c.api_id);
        if !options.dry_run
            && let Err(e) = client.create_component(&c.without_relations_to(deferred)).await
        {
            eprintln!("    FAILED: {e:#}");
            failures += 1;
        }
    }

    if !plan.create.is_empty() || !plan.update.is_empty() || !plan.skipped_models.is_empty() {
        eprintln!("content types{dry}:");
    }
    for reason in &plan.skipped_models {
        eprintln!("  skip {reason}");
    }
    // Pass 1: create new models without their relations to models created
    // later (handles cycles such as A <-> B; self-relations are kept).
    for (model, deferred) in &plan.create {
        let note = if deferred.is_empty() { "" } else { " (relations added in a second pass)" };
        eprintln!("  create {}{note}", model.api_id);
        if !options.dry_run
            && let Err(e) = client.create_model(&model.without_relations_to(deferred)).await
        {
            eprintln!("    FAILED: {e:#}");
            failures += 1;
        }
    }
    // Pass 2: full definitions, components first.
    for (c, verb) in &plan.update_components {
        eprintln!("  {verb} component {}", c.api_id);
        if !options.dry_run
            && let Err(e) = client.update_component(c).await
        {
            eprintln!("    FAILED: {e:#}");
            failures += 1;
        }
    }
    for (model, verb) in &plan.update {
        eprintln!("  {verb} {}", model.api_id);
        if !options.dry_run
            && let Err(e) = client.update_model(model).await
        {
            eprintln!("    FAILED: {e:#}");
            failures += 1;
        }
    }

    if !plan.create_users.is_empty() || !plan.update_users.is_empty() || !plan.skipped_users.is_empty() {
        eprintln!("users{dry}:");
    }
    for reason in &plan.skipped_users {
        eprintln!("  skip {reason}");
    }
    let mut passwords = vec![];
    for user in &plan.create_users {
        eprintln!("  create {} ({})", user.email, user.role);
        if options.dry_run {
            continue;
        }
        let password = user.password.clone().unwrap_or_else(random_password);
        match client.create_user(user, &password).await {
            Ok(()) if user.password.is_none() => passwords.push((user.email.clone(), password)),
            Ok(()) => {}
            Err(e) => {
                eprintln!("    FAILED: {e:#}");
                failures += 1;
            }
        }
    }
    for (id, user) in &plan.update_users {
        eprintln!("  update {} ({})", user.email, user.role);
        if !options.dry_run
            && let Err(e) = client.update_user(id, user).await
        {
            eprintln!("    FAILED: {e:#}");
            failures += 1;
        }
    }
    if let (false, Some(path)) = (passwords.is_empty(), options.passwords_out.as_deref()) {
        write_passwords(path, &passwords)?;
        eprintln!(
            "  wrote {} temporary password(s) to {path} (mode 600): share each privately, then delete the file",
            passwords.len()
        );
    }
    if failures > 0 {
        bail!("{failures} item(s) failed");
    }
    eprintln!("done");
    Ok(())
}

async fn plan(
    client: &Client,
    components: Vec<ComponentDoc>,
    models: Vec<ModelDef>,
    users: Vec<UserDef>,
    options: &ImportOptions,
) -> Result<Plan> {
    let mut plan = Plan::default();

    let existing_types: BTreeSet<String> = if models.is_empty() && components.is_empty() {
        BTreeSet::new()
    } else {
        client.models().await?.into_iter().map(|m| m.api_id).collect()
    };
    // Content types available as relation targets once the file is imported.
    let all_types: BTreeSet<String> =
        existing_types.iter().cloned().chain(models.iter().map(|m| m.api_id.clone())).collect();

    if !components.is_empty() {
        let existing: BTreeSet<String> = client.components().await?.into_iter().map(|c| c.api_id).collect();
        let mut seen = BTreeSet::new();
        let mut new_components = vec![];
        for c in components {
            if !seen.insert(c.api_id.clone()) {
                bail!("component {} appears twice in the file", c.api_id);
            }
            if !existing.contains(&c.api_id) {
                new_components.push(c);
            } else if options.update {
                plan.update_components.push((c, "update"));
            } else {
                plan.skipped_components.push(format!("{}: exists (use --update to replace its definition)", c.api_id));
            }
        }
        let available: BTreeSet<&str> =
            existing.iter().map(String::as_str).chain(new_components.iter().map(|c| c.api_id.as_str())).collect();
        for c in new_components.iter().chain(plan.update_components.iter().map(|(c, _)| c)) {
            if let Some(used) = component_refs(&c.fields).find(|u| !available.contains(u)) {
                bail!("component {} uses component `{used}`, which is neither on the server nor in the file", c.api_id);
            }
            if let Some(target) = relation_targets(&c.fields).find(|t| !all_types.contains(*t)) {
                bail!("component {} has a relation to `{target}`, which is neither on the server nor in the file", c.api_id);
            }
        }
        // Nested components first (cycles are refused by the server anyway).
        let mut done: BTreeSet<String> = existing.clone();
        let mut remaining = new_components;
        while !remaining.is_empty() {
            let (ready, blocked): (Vec<_>, Vec<_>) =
                remaining.into_iter().partition(|c| component_refs(&c.fields).all(|u| done.contains(u)));
            if ready.is_empty() {
                bail!("components {} reference each other in a cycle", blocked.iter().map(|c| c.api_id.as_str()).collect::<Vec<_>>().join(", "));
            }
            for c in ready {
                done.insert(c.api_id.clone());
                // Relations to content types created later are added in pass 2.
                let deferred: BTreeSet<String> = relation_targets(&c.fields)
                    .filter(|t| !existing_types.contains(*t))
                    .map(String::from)
                    .collect();
                if !deferred.is_empty() {
                    plan.update_components.insert(0, (c.clone(), "complete"));
                }
                plan.create_components.push((c, deferred));
            }
            remaining = blocked;
        }
    }

    if !models.is_empty() {
        let existing = existing_types.clone();
        let mut seen = BTreeSet::new();
        let mut new_models = vec![];
        for m in models {
            if !seen.insert(m.api_id.clone()) {
                bail!("content type {} appears twice in the file", m.api_id);
            }
            if BUILTIN_TYPES.contains(&m.api_id.as_str()) {
                plan.skipped_models.push(format!("{}: built-in", m.api_id));
            } else if !existing.contains(&m.api_id) {
                new_models.push(m);
            } else if options.update {
                plan.update.push((m, "update"));
            } else {
                plan.skipped_models.push(format!("{}: exists (use --update to replace its definition)", m.api_id));
            }
        }
        // Every relation must target a model that exists or is being created.
        let available: BTreeSet<&str> =
            existing.iter().map(String::as_str).chain(new_models.iter().map(|m| m.api_id.as_str())).collect();
        for m in new_models.iter().chain(plan.update.iter().map(|(m, _)| m)) {
            if let Some(target) = m.relation_targets().find(|t| !available.contains(t)) {
                bail!("{} has a relation to `{target}`, which is neither on the server nor in the file", m.api_id);
            }
        }
        let mut pending: BTreeSet<String> = new_models.iter().map(|m| m.api_id.clone()).collect();
        let mut completions = vec![];
        for m in new_models {
            pending.remove(&m.api_id);
            let deferred: BTreeSet<String> =
                m.relation_targets().filter(|t| pending.contains(*t)).map(String::from).collect();
            if !deferred.is_empty() {
                completions.push((m.clone(), "complete"));
            }
            plan.create.push((m, deferred));
        }
        completions.append(&mut plan.update);
        plan.update = completions;
    }

    if !users.is_empty() {
        let existing: HashMap<String, String> =
            client.users().await?.into_iter().map(|u| (u.def.email.to_lowercase(), u.id)).collect();
        let mut seen = BTreeSet::new();
        for mut u in users {
            u.email = u.email.trim().to_lowercase();
            if u.email.is_empty() || u.role.is_empty() {
                bail!("every user needs `email` and `role`");
            }
            if !seen.insert(u.email.clone()) {
                bail!("user {} appears twice in the file", u.email);
            }
            match existing.get(&u.email) {
                None => plan.create_users.push(u),
                Some(id) if options.update => plan.update_users.push((id.clone(), u)),
                Some(_) => plan
                    .skipped_users
                    .push(format!("{}: exists (use --update to change name, role, scope and status)", u.email)),
            }
        }
        let need = plan.create_users.iter().filter(|u| u.password.is_none()).count();
        if need > 0 && !options.dry_run && options.passwords_out.is_none() {
            bail!("{need} new user(s) will get random passwords: pass --passwords-out FILE to receive them");
        }
    }
    Ok(plan)
}

fn random_password() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 18];
    getrandom::fill(&mut bytes).expect("OS randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn write_passwords(path: &str, passwords: &[(String, String)]) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).with_context(|| format!("cannot create {path}"))?;
    // Also when the file already existed with wider permissions.
    #[cfg(unix)]
    std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    writeln!(file, "# Temporary passwords of imported users. Share each one privately, then delete this file.")?;
    for (email, password) in passwords {
        writeln!(file, "{email}\t{password}")?;
    }
    Ok(())
}
