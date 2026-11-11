//! Skill management: publish a version of a skill (a `SKILL.md` with YAML
//! frontmatter plus a reference file), fetch it back with its file contents,
//! and list the project's skills. `YamlTool`, the SDK's small YAML utility,
//! builds and parses the frontmatter and shows a deep merge of two YAML
//! documents along the way.
//!
//! A skill is a set of files an agent loads on demand; Langfuse versions it
//! like a prompt. Its name and description come from the frontmatter of
//! `SKILL.md`, not from the request.
//!
//! **Writes data:** every run adds a version to the skill
//! `rust-sdk-demo-skill`, which is kept. Delete versions with
//! `client.skills.delete_version(name, version)`; the API has no call that
//! deletes a whole skill.
//!
//! **Docs:** <https://langfuse.com/docs/skill-management/overview>,
//! <https://langfuse.com/docs/skill-management/version-control>
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example skills
//! ```

use langfuse_rust_sdk::api::skills::{GetSkillParams, ListSkillsParams};
use langfuse_rust_sdk::models::skills::{CreateSkillVersionRequest, SkillVersionFileCreateInput};
use langfuse_rust_sdk::tools::YamlTool;
use langfuse_rust_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    Ok(LangfuseClient::new(ClientConfig::from_env()?)?)
}

/// `SKILL.md`: YAML frontmatter between `---` lines, then the instructions.
/// `name` and `description` are required; the description tells an agent
/// what the skill does and when to use it.
fn build_skill_md(
    name: &str,
    description: &str,
    body: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let frontmatter = serde_json::json!({
        "name": name,
        "description": description,
    });
    let yaml = YamlTool::to_string(&frontmatter)?;
    Ok(format!("---\n{}---\n\n{body}", yaml))
}

/// A supporting file of the skill: a markdown checklist, with the same data
/// as YAML in a trailing comment.
fn build_checklist_md(items: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let data = serde_json::json!({ "checklist": items });
    let yaml_repr = YamlTool::to_string(&data)?;
    let mut md = String::from("# Review Checklist\n\n");
    for item in items {
        md.push_str(&format!("- [ ] {item}\n"));
    }
    md.push_str(&format!("\n<!-- Generated from YAML:\n{yaml_repr}-->\n"));
    Ok(md)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let client = build_client()?;
    let skill_name = "rust-sdk-demo-skill";

    // 1. Build the two files locally. Nothing is sent yet.
    println!("=== Building skill files with YamlTool ===\n");

    let skill_md = build_skill_md(
        skill_name,
        "A demo code review skill created from the Rust SDK.",
        "# Code Review Skill\n\n\
         You are a code reviewer. Follow these principles:\n\n\
         1. Check for correctness bugs first\n\
         2. Then look for simplification opportunities\n\
         3. Only suggest changes you are confident about\n",
    )?;
    println!("Generated SKILL.md ({} bytes)", skill_md.len());

    let checklist_md = build_checklist_md(&[
        "No off-by-one errors",
        "Error handling is complete",
        "No unused variables",
        "No security vulnerabilities",
    ])?;
    println!("Generated checklist.md ({} bytes)", checklist_md.len());

    // Check that the frontmatter between the two `---` lines parses as YAML.
    let fm_yaml = &skill_md[skill_md.find("---").unwrap() + 4..skill_md.find("\n---").unwrap()];
    YamlTool::validate(fm_yaml)?;
    println!("Frontmatter YAML validated successfully");

    // 2. Publish. A version holds the complete file set: a file left out of
    //    the request is not part of the new version. The new version gets the
    //    `latest` label; `production` or `staging` are assigned separately
    //    (`client.skills.set_version_labels`).
    println!("\n=== Creating skill version ===\n");

    let created = client
        .skills
        .create_version(&CreateSkillVersionRequest {
            files: vec![
                SkillVersionFileCreateInput::content("SKILL.md", skill_md),
                SkillVersionFileCreateInput::content("references/checklist.md", checklist_md),
            ],
            commit_message: Some("Code review skill with YAML-built frontmatter".into()),
        })
        .await?;
    println!(
        "Created: {} v{} (labels: {:?})",
        created.name, created.version, created.labels
    );
    println!("Files: {}", created.files.len());
    for f in &created.files {
        println!("  - {} ({} bytes)", f.path, f.content_length);
    }

    // 3. Fetch it back. Without a label or version the API returns the
    //    `production` version (404 if there is none), so ask for `latest`.
    //    File contents are only included with `include_content`.
    println!("\n=== Fetching skill and extracting frontmatter ===\n");

    let fetched = client
        .skills
        .get(
            skill_name,
            &GetSkillParams {
                label: Some("latest".into()),
                include_content: Some(true),
                ..Default::default()
            },
        )
        .await?;

    let skill_file = fetched
        .files
        .iter()
        .find(|f| f.path == "SKILL.md")
        .expect("SKILL.md not found");

    let content = skill_file.content.as_deref().expect("No content returned");
    let (frontmatter, body) = YamlTool::extract_frontmatter(content)?;

    println!("Extracted frontmatter:");
    let fm_name = YamlTool::get_field(&frontmatter, "name")?;
    let fm_desc = YamlTool::get_field(&frontmatter, "description")?;
    println!("  name: {}", fm_name.as_str().unwrap_or("?"));
    println!("  description: {}", fm_desc.as_str().unwrap_or("?"));
    println!("  keys: {:?}", YamlTool::keys(&frontmatter));

    println!(
        "\nBody preview: {}...",
        body.chars().take(80).collect::<String>()
    );

    // 4. Any `Serialize` value can be printed as YAML.
    println!("\n=== Skill metadata as YAML ===\n");

    let metadata = serde_json::json!({
        "name": fetched.name,
        "version": fetched.version,
        "labels": fetched.labels,
        "tags": fetched.tags,
        "files": fetched.files.iter().map(|f| {
            serde_json::json!({
                "path": f.path,
                "size": f.content_length,
                "sha256": f.sha256_hash,
            })
        }).collect::<Vec<_>>(),
    });
    let yaml_output = YamlTool::to_string(&metadata)?;
    println!("{yaml_output}");

    // 5. Deep merge of two YAML documents (not a Langfuse API call): keys of
    //    the overlay win, nested mappings are merged key by key.
    println!("=== Merging skill config overlay ===\n");

    let mut base_config = YamlTool::parse_value(
        "review:\n  max_findings: 10\n  severity: high\n  auto_fix: false\n",
    )?;
    let overlay = YamlTool::parse_value(
        "review:\n  auto_fix: true\n  timeout: 60\noutput:\n  format: markdown\n",
    )?;
    YamlTool::merge(&mut base_config, &overlay);

    println!("Merged config:");
    println!("{}", YamlTool::to_string(&base_config)?);

    // 6. One entry per skill, with its latest and production versions
    //    (page-based pagination).
    println!("=== All skills ===\n");

    let skills = client
        .skills
        .list(&ListSkillsParams {
            limit: Some(10),
            ..Default::default()
        })
        .await?;
    println!(
        "{} of {} skills:",
        skills.data.len(),
        skills.meta.total_items
    );
    for s in &skills.data {
        println!(
            "  - {} v{} (tags: {:?}, production: {:?})",
            s.name, s.latest_version, s.tags, s.production_version
        );
    }
    if skills.meta.has_next_page {
        println!("  ... more on the next page");
    }

    Ok(())
}
