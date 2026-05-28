use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::CoreError;

/// A loaded skill definition
#[derive(Debug, Clone)]
pub struct SkillDef {
    pub name: String,
    pub description: String,
    pub instructions: String,
}

/// Auto-discovers and serves skills from a directory
#[derive(Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDef>,
}

impl SkillRegistry {
    /// Discover skills from a directory. Each subdirectory should contain SKILL.md.
    pub fn discover(dir: &Path) -> Result<Self, CoreError> {
        let mut skills = HashMap::new();

        if !dir.exists() {
            tracing::warn!("Skills directory not found: {:?}", dir);
            return Ok(Self { skills });
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let skill_path = path.join("SKILL.md");
            if !skill_path.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&skill_path)?;
            match Self::parse_skill(&content, &path) {
                Ok(skill) => {
                    tracing::info!("Discovered skill: {} ({})", skill.name, skill.description);
                    skills.insert(skill.name.clone(), skill);
                }
                Err(e) => {
                    tracing::warn!("Failed to load skill from {:?}: {e}", path);
                }
            }
        }

        Ok(Self { skills })
    }

    /// Parse SKILL.md with YAML frontmatter
    fn parse_skill(content: &str, path: &Path) -> Result<SkillDef, CoreError> {
        // Extract frontmatter between --- markers
        let content_trimmed = content.trim();
        if !content_trimmed.starts_with("---") {
            return Err(CoreError::Skill(format!(
                "Missing frontmatter in {:?}",
                path
            )));
        }

        let without_open = &content_trimmed[3..];
        let end = without_open
            .find("---")
            .ok_or_else(|| CoreError::Skill(format!("Unclosed frontmatter in {:?}", path)))?;

        let frontmatter_str = &without_open[..end];
        let instructions = without_open[end + 3..].trim().to_string();

        // Parse YAML frontmatter
        let frontmatter: HashMap<String, String> = serde_yaml::from_str(frontmatter_str)
            .map_err(|e| {
                CoreError::Skill(format!("Invalid frontmatter in {:?}: {e}", path))
            })?;

        let name = frontmatter
            .get("name")
            .ok_or_else(|| {
                CoreError::Skill(format!("Missing 'name' in frontmatter: {:?}", path))
            })?
            .clone();

        let description = frontmatter.get("description").cloned().unwrap_or_default();

        Ok(SkillDef {
            name,
            description,
            instructions,
        })
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&SkillDef> {
        self.skills.get(name)
    }

    /// List all discovered skills
    pub fn list(&self) -> Vec<&SkillDef> {
        self.skills.values().collect()
    }

    /// Merge skills from another registry into this one
    pub fn merge(&mut self, other: Self) {
        for (name, skill) in other.skills {
            self.skills.entry(name).or_insert(skill);
        }
    }

    /// Build the complete system prompt with ALL skills in this registry injected
    pub fn build_system_prompt(&self, role: &str) -> String {
        let mut prompt = role.to_string();
        prompt.push_str("\n\n");

        let all_skills: Vec<&SkillDef> = self.skills.values().collect();
        if !all_skills.is_empty() {
prompt.push_str("## Available Skills\n\n");
        prompt.push_str("You have the following skills available. Choose the right skill based on the task and follow its instructions carefully.\n\n");

            for skill in &all_skills {
                prompt.push_str(&format!("### {}\n", skill.name));
                if !skill.description.is_empty() {
                    prompt.push_str(&format!("{}\n\n", skill.description));
                }
                prompt.push_str(&format!("{}\n\n", skill.instructions));
            }
        }

        // Communication rules for ALL agents
        prompt.push_str(r#"
## Communication Rules (Feishu)

Send messages to the Feishu group chat ONLY when necessary. Do NOT broadcast every thought.

### Thread-Based Communication

When working on a task, you may receive a message starting with [Thread: xxx].
This means you should send all task-related replies in that thread by including
"thread_id": "xxx" in your send_feishu_message calls.

- General conversation: no thread_id needed (sends to main channel)
- Task work: always include the thread_id
- @mention other agents in the thread to keep all context together
- When in doubt, reply in the same thread you received the message from

### When to send a message (use send_feishu_message tool):
- You completed a user's request and have a final result
- You need to request information or action from another agent via @mention
- You encountered a blocker you cannot resolve alone
- The user explicitly asked for a progress update

### When NOT to send a message:
- Internal reasoning, analysis, or planning (LLM will remember these)
- Minor intermediate progress (unless the user asked for real-time tracking)
- While waiting for another agent's reply (wait for it, then respond once)

### Style:
- Responses to users: concise, complete, lead with the conclusion
- Messages to other agents: clear what you need from them
- One message per update — do not split into multiple messages
- If it fits in one sentence, do not write three paragraphs.
"#);

        prompt
    }

    /// Start a file watcher that hot-reloads skills when SKILL.md files change
    pub fn start_watcher(
        registry: Arc<RwLock<Self>>,
        watch_dirs: Vec<PathBuf>,
    ) -> Result<tokio::task::JoinHandle<()>, CoreError> {
        use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc;

        let (tx, rx) = mpsc::channel::<Result<notify::Event, notify::Error>>();
        let mut watcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| CoreError::Skill(format!("Create watcher: {e}")))?;

        for dir in &watch_dirs {
            if dir.exists() {
                watcher
                    .watch(dir, RecursiveMode::Recursive)
                    .map_err(|e| CoreError::Skill(format!("Watch {:?}: {e}", dir)))?;
                tracing::info!("[Watcher] Watching skill directory: {:?}", dir);
            } else {
                tracing::debug!("[Watcher] Skipping non-existent directory: {:?}", dir);
            }
        }

        // Move watcher into the blocking task so it stays alive
        let handle = tokio::task::spawn_blocking(move || {
            let _watcher = watcher; // keep alive
            for res in rx {
                match res {
                    Ok(event) => {
                        let is_skill_change = matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) && event.paths.iter().any(|p| {
                            p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md")
                        });

                        if is_skill_change {
                            tracing::info!("[Watcher] SKILL.md changed: {:?}, hot-reloading...", event.paths);
                            let mut reg = registry.blocking_write();
                            // Clear and re-discover from all watched directories
                            reg.skills.clear();
                            for dir in &watch_dirs {
                                if dir.exists() {
                                    if let Ok(fresh) = Self::discover(dir) {
                                        reg.merge(fresh);
                                    }
                                }
                            }
                            tracing::info!("[Watcher] Hot-reload complete — {} skills loaded", reg.skills.len());
                        }
                    }
                    Err(e) => {
                        tracing::error!("[Watcher] Error: {e}");
                    }
                }
            }
        });

        Ok(handle)
    }
}

fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Get the global skills directory path (~/.config/OpenTeam/skills/)
pub fn global_skills_dir() -> std::path::PathBuf {
    home_dir().join(".config/OpenTeam/skills")
}

/// Get the assistant skills directory path (~/.config/OpenTeam/assistant/skills/)
pub fn assistant_skills_dir() -> std::path::PathBuf {
    home_dir().join(".config/OpenTeam/assistant/skills")
}

/// Get the global agents directory path (~/.config/OpenTeam/agents/)
pub fn global_agents_dir() -> std::path::PathBuf {
    home_dir().join(".config/OpenTeam/agents")
}

/// Built-in skills embedded at compile time, released to global config on startup
const BUILT_IN_SKILLS: &[(&str, &str, &str)] = &[
    (
        "feishu-doc",
        "Create and manage Feishu documents and access Feishu MCP tools",
        "## Available Tools (via Feishu Remote MCP)\n\n\
         The following tools are available through the standard Feishu MCP service. \
         They are automatically discovered and don't require separate configuration.\n\n\
         ### Document Tools\n\
         - `create-doc` — Create a new cloud document (requires: title)\n\
         - `fetch-doc` — Read document content (requires: docID)\n\
         - `search-doc` — Search documents by keyword (requires: query)\n\
         - `update-doc` — Update document content (requires: docID + content)\n\
         - `list-docs` — List documents in a knowledge space\n\
         - `get-comments` — View document comments (requires: docID)\n\
         - `add-comments` — Add a comment (requires: docID + content)\n\n\
         ### User Tools\n\
         - `search-user` — Search for colleagues (requires: query)\n\
         - `get-user` — Get user details (requires: userID)\n\n\
         ### File Tools\n\
         - `fetch-file` — Retrieve file content (requires: fileToken)\n\n\
         ## Instructions\n\n\
         You have access to Feishu cloud services via MCP tools. **Always use Feishu cloud documents \
         instead of local files or markdown when writing documentation, meeting notes, proposals, or any \
         content that needs to be shared with others.**\n\n\
         ## Best Practices\n\n\
         1. **Cloud-first**: Always use Feishu cloud documents for any content needing sharing\n\
         2. **Clear titles**: Use descriptive titles so documents can be found via search\n\
         3. **Share URLs**: After creating a document, share the returned URL with the user\n\
         4. **Search first**: Search for existing documents before creating new ones\n\
         5. **Review**: Use `fetch-doc` + `get-comments` to review documents and feedback",
    ),
];

/// Release built-in skills to the global skills directory if they don't exist.
/// Called once at startup before skill discovery.
pub fn release_builtin_skills() -> Result<(), CoreError> {
    let global_dir = global_skills_dir();

    for (name, description, instructions) in BUILT_IN_SKILLS {
        let skill_dir = global_dir.join(name);
        let skill_file = skill_dir.join("SKILL.md");

        if skill_file.exists() {
            tracing::debug!("Built-in skill already exists, skipping: {name}");
            continue;
        }

        std::fs::create_dir_all(&skill_dir)?;

        let frontmatter = format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n"
        );
        let content = format!("{frontmatter}{instructions}");
        std::fs::write(&skill_file, &content)?;

        tracing::info!("Released built-in skill: {name} -> {:?}", skill_file);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_discover_empty_dir() {
        let tmp = std::env::temp_dir().join("feishu_skills_empty_test");
        let _ = std::fs::create_dir_all(&tmp);
        let registry = SkillRegistry::discover(&tmp).unwrap();
        assert!(registry.list().is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_parse_valid_skill() {
        let content = r#"---
name: test-skill
description: A test skill
---
# Test Skill
Do something useful."#;
        let tmp = std::env::temp_dir().join("feishu_skill_test");
        let _ = std::fs::create_dir_all(&tmp.join("test-skill"));
        let mut f = std::fs::File::create(&tmp.join("test-skill").join("SKILL.md")).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        let registry = SkillRegistry::discover(&tmp).unwrap();
        let skill = registry.get("test-skill").unwrap();
        assert_eq!(skill.name, "test-skill");
        assert!(skill.instructions.contains("Do something useful"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_build_system_prompt() {
        let content = r#"---
name: feishu-doc
description: Manage docs
---
Create Feishu documents."#;
        let tmp = std::env::temp_dir().join("feishu_skill_prompt_test");
        let _ = std::fs::create_dir_all(&tmp.join("feishu-doc"));
        let mut f = std::fs::File::create(&tmp.join("feishu-doc").join("SKILL.md")).unwrap();
        f.write_all(content.as_bytes()).unwrap();

        let registry = SkillRegistry::discover(&tmp).unwrap();
        let prompt = registry.build_system_prompt("You are a product manager");
        assert!(prompt.contains("You are a product manager"));
        assert!(prompt.contains("feishu-doc"));
        assert!(prompt.contains("Create Feishu documents"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
