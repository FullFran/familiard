//! The setup skill carries copies of the docs, so that it still works once copied elsewhere.
use std::fs;
use std::path::Path;

#[test]
fn the_skill_references_match_the_docs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: String| fs::read_to_string(root.join(path)).unwrap().replace("\r\n", "\n");
    for doc in ["pets.md", "agents.md", "browser.md"] {
        assert_eq!(
            read(format!("docs/{doc}")),
            read(format!("skills/familiard-setup/references/{doc}")),
            "copy docs/{doc} into the skill's references"
        );
    }
}
