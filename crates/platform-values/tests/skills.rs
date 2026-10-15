use platform_values::{SkillsError, normalize_skills};

#[test]
fn trims_collapses_and_deduplicates_ignoring_case() {
    let out = normalize_skills([" Rust ", "rust", "", "  SQL   Server "]).unwrap();
    assert_eq!(out, ["Rust", "SQL Server"]);
}

#[test]
fn too_long_skill_is_rejected() {
    assert_eq!(normalize_skills(["x".repeat(51)]), Err(SkillsError::TooLong));
}

#[test]
fn more_than_fifty_skills_are_rejected() {
    let many: Vec<String> = (0..51).map(|n| format!("s{n}")).collect();
    assert_eq!(normalize_skills(many), Err(SkillsError::TooMany));
}
