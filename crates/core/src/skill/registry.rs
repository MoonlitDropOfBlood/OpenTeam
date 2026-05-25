use std::collections::HashMap;
use std::path::Path;
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
        if all_skills.is_empty() {
            return prompt;
        }

        prompt.push_str("## 可用技能\n\n");
        prompt.push_str("你有以下技能可用，根据需求选择合适的技能，严格按照技能说明执行。\n\n");

        for skill in &all_skills {
            prompt.push_str(&format!("### {}\n", skill.name));
            if !skill.description.is_empty() {
                prompt.push_str(&format!("{}\n\n", skill.description));
            }
            prompt.push_str(&format!("{}\n\n", skill.instructions));
        }

        prompt
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
