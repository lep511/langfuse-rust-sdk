//! Create a skill version, fetch it back, and use YamlTool to
//! build frontmatter, extract it, and convert skill data to YAML.
//!
//! ```sh
//! LANGFUSE_PUBLIC_KEY=pk-lf-... LANGFUSE_SECRET_KEY=sk-lf-... \
//!   cargo run --example skills
//! ```

use langfuse_sdk::api::skills::{GetSkillParams, ListSkillsParams};
use langfuse_sdk::models::skills::{CreateSkillVersionRequest, SkillVersionFileInput};
use langfuse_sdk::tools::YamlTool;
use langfuse_sdk::{ClientConfig, LangfuseClient};

fn build_client() -> Result<LangfuseClient, Box<dyn std::error::Error>> {
    let config = ClientConfig::builder(
        std::env::var("LANGFUSE_PUBLIC_KEY")?,
        std::env::var("LANGFUSE_SECRET_KEY")?,
    )
    .base_url(
        std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".into()),
    )
    .build()?;
    Ok(LangfuseClient::new(config)?)
}

/// Build SKILL.md content with YAML frontmatter using YamlTool.
fn build_skill_md(name: &str, description: &str, body: &str) -> Result<String, Box<dyn std::error::Error>> {
    let frontmatter = serde_json::json!({
        "name": name,
        "description": description,
    });
    let yaml = YamlTool::to_string(&frontmatter)?;
    Ok(format!("---\n{}---\n\n{body}", yaml))
}

/// Build a reference file from a YAML-defined checklist.
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

    // 1. Build SKILL.md using YamlTool to serialize the frontmatter
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

    // Validate both files are valid documents
    let fm_yaml = &skill_md[skill_md.find("---").unwrap() + 4..skill_md.find("\n---").unwrap()];
    YamlTool::validate(fm_yaml)?;
    println!("Frontmatter YAML validated successfully");

    // 2. Create the skill version
    println!("\n=== Creating skill version ===\n");

    let created = client
        .skills
        .create_version(&CreateSkillVersionRequest {
            files: vec![
                SkillVersionFileInput {
                    path: "SKILL.md".into(),
                    content: skill_md,
                },
                SkillVersionFileInput {
                    path: "references/checklist.md".into(),
                    content: checklist_md,
                },
            ],
            commit_message: Some("Code review skill with YAML-built frontmatter".into()),
        })
        .await?;
    println!("Created: {} v{} (labels: {:?})", created.name, created.version, created.labels);
    println!("Files: {}", created.files.len());
    for f in &created.files {
        println!("  - {} ({} bytes)", f.path, f.content_length);
    }

    // 3. Fetch the skill and extract frontmatter with YamlTool
    println!("\n=== Fetching skill and extracting frontmatter ===\n");

    let fetched = client
        .skills
        .get(
            skill_name,
            &GetSkillParams {
                label: Some("latest".into()),
                include_content: Some(true),
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

    println!("\nBody preview: {}...", &body[..body.len().min(80)]);

    // 4. Convert skill metadata to YAML for display
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

    // 5. Deep-merge an overlay config onto skill settings
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

    // 6. List all skills
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
        println!("  - {} (tags: {:?})", s.name, s.tags);
    }

    Ok(())
}
