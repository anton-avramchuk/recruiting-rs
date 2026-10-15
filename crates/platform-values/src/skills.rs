//! Нормализация списка навыков: общая для кандидатов и требований вакансий.

const MAX_SKILLS: usize = 50;
const MAX_SKILL_LEN: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SkillsError {
    #[error("skill is longer than 50 characters")]
    TooLong,
    #[error("more than 50 skills")]
    TooMany,
}

fn collapse(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Навыки без лишних пробелов, пустых и повторов (регистр не различается,
/// остаётся первое написание).
pub fn normalize_skills<I, S>(skills: I) -> Result<Vec<String>, SkillsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for raw in skills {
        let skill = collapse(raw.as_ref());
        if skill.is_empty() {
            continue;
        }
        if skill.chars().count() > MAX_SKILL_LEN {
            return Err(SkillsError::TooLong);
        }
        if !out.iter().any(|s| s.to_lowercase() == skill.to_lowercase()) {
            out.push(skill);
        }
    }
    if out.len() > MAX_SKILLS {
        return Err(SkillsError::TooMany);
    }
    Ok(out)
}
