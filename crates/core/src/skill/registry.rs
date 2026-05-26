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
            prompt.push_str("## 可用技能\n\n");
            prompt.push_str("你有以下技能可用，根据需求选择合适的技能，严格按照技能说明执行。\n\n");

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

**When to send a message (use send_feishu_message tool):**
- You completed a user's request and have a final result
- You need to request information or action from another agent via @mention
- You encountered a blocker you cannot resolve alone
- The user explicitly asked for a progress update

**When NOT to send a message:**
- Internal reasoning, analysis, or planning (LLM will remember these)
- Minor intermediate progress (unless the user asked for real-time tracking)
- While waiting for another agent's reply (wait for it, then respond once)

**Style:**
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
        let prompt = registry.build_system_prompt("你是产品经理");
        assert!(prompt.contains("你是产品经理"));
        assert!(prompt.contains("feishu-doc"));
        assert!(prompt.contains("Create Feishu documents"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
